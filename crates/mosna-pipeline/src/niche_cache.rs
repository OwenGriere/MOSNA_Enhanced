//! The intermediate results step 3 reuses instead of recomputing.
//!
//! # What is cached, and what that costs
//!
//! Three stages dominate a niche analysis: aggregating every neighbourhood into
//! a feature vector, projecting that matrix, and partitioning the projection.
//! Each is a pure function of the stage before it and of a handful of settings,
//! so each can be written to a file named after those settings and read back
//! when they are unchanged. A second run that only moves `resolution` then pays
//! for the clustering alone, where it used to pay for all three.
//!
//! The files live beside `net_dir_mosna` under `temp`, because that is what
//! `clear-temporary` removes: a cache the user cannot drop is a liability, and
//! everything here is rebuildable by definition.
//!
//! # Why the file says where it came from
//!
//! Each name carries the settings of its own stage and no more:
//! `red_umap_dim2_nneigh20_euclidean_mindist00.parquet` says what the
//! projection is, not what was projected. Two aggregations of the same cohort
//! under different columns therefore meet at that name. What separates them is
//! written inside the file, in the parquet footer — [`SOURCE_KEY`] — and a
//! reader that wanted a different source recomputes instead of reusing. The
//! name stays readable; the cache stays correct.
//!
//! # Why the row count is checked as well as the name
//!
//! The names carry the settings that change a result — see
//! [`fn@mosna_config::model::niche_params::NicheParams::features_stem`] — but
//! they cannot carry the cohort. A working directory whose `net_dir` gained a
//! sample has the same settings and a different answer, and would otherwise
//! read back a feature table describing cells that are no longer all of them.
//! So every cache hit is checked against the number of rows it should have, and
//! a file of the wrong height is recomputed rather than trusted. The check is a
//! parquet footer read, which costs nothing next to what it guards.

use std::path::{Path, PathBuf};

use mosna_io::read::get_opener::read_table;
use mosna_io::read::read_parquet::read_parquet_key;
use mosna_io::write::write_parquet::write_parquet_with_metadata;
use mosna_io::{Extension, Table};

use crate::error::{create_dir_all, Result};
use crate::niche_runs::{paths, RunNumbers};

/// The footer key naming the stage a cached file was computed from.
pub const SOURCE_KEY: &str = "mosna.source";

/// Where the caches live, under a working directory.
///
/// The layout is the catalogue's — see [`crate::niche_runs::paths`] — so this
/// only has to put the working directory in front of it and make the
/// directories on the way.
#[derive(Debug, Clone)]
pub struct Caches {
    working_dir: PathBuf,
}

impl Caches {
    pub fn under(working_dir: &Path) -> Self {
        Self {
            working_dir: working_dir.to_path_buf(),
        }
    }

    pub fn features_path(&self, numbers: &RunNumbers) -> PathBuf {
        self.working_dir.join(paths::features(numbers.var_aggreg))
    }

    pub fn reduction_path(&self, numbers: &RunNumbers) -> PathBuf {
        self.working_dir
            .join(paths::reduction(numbers.var_aggreg, numbers.reduction))
    }

    pub fn clustering_path(&self, numbers: &RunNumbers) -> PathBuf {
        self.working_dir.join(paths::clustering(
            numbers.var_aggreg,
            numbers.reduction,
            numbers.clustering,
        ))
    }

    /// Create the directories this run is about to write into.
    ///
    /// One call per run rather than one at start-up: which directories exist
    /// depends on the numbers the catalogue handed out, and those are not known
    /// until the settings have been resolved.
    pub fn prepare(&self, numbers: &RunNumbers) -> Result<()> {
        create_dir_all(
            self.working_dir
                .join(paths::reduction_dir(numbers.var_aggreg, numbers.reduction)),
        )
    }

    /// `path` as the catalogue records it: relative to the working directory,
    /// so the record survives the directory being moved.
    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.working_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

/// Whether a cached file was computed from `source`.
/// Whether a cached file was computed from `source`.
///
/// A file with no recorded source is one this version did not write, and is not
/// reused: guessing would be the one mistake this whole mechanism exists to
/// prevent.
pub fn came_from(path: &Path, source: &str) -> bool {
    matches!(read_parquet_key(path, SOURCE_KEY), Ok(Some(recorded)) if recorded == source)
}

/// A cached projection, if one is on disk, came from `source`, and has the
/// expected height.
///
/// Returns the matrix and its width. `None` covers every reason not to reuse —
/// absent, unreadable, from something else, or the wrong shape — because they
/// all lead to the same place: compute it again.
pub fn read_matrix(path: &Path, source: &str, expected_rows: usize) -> Option<(Vec<f64>, usize)> {
    if !path.is_file() || !came_from(path, source) {
        return None;
    }
    let table = read_table(path, Extension::Parquet).ok()?;
    if table.n_rows() != expected_rows || table.n_columns() == 0 {
        return None;
    }

    // Columns are named `dim_0`, `dim_1`, … and read back in that order rather
    // than in the file's, so a reader that reorders them cannot transpose the
    // embedding silently.
    let width = table.n_columns();
    let mut columns = Vec::with_capacity(width);
    for index in 0..width {
        columns.push(table.f64_column(&format!("dim_{index}")).ok()?);
    }

    let mut values = vec![0.0; expected_rows * width];
    for (index, column) in columns.iter().enumerate() {
        for (row, value) in column.iter().enumerate() {
            values[row * width + index] = *value;
        }
    }
    Some((values, width))
}

/// Write a table for the next run, recording what it was computed from.
///
/// The feature table goes through here rather than through `write_parquet`
/// directly: its file is now called `var_aggreg_1`, which says nothing about
/// what it holds, so the footer has to.
pub fn write_table(path: &Path, source: &str, table: &Table) -> Result<()> {
    write_parquet_with_metadata(table, path, &[(SOURCE_KEY.to_string(), source.to_string())])?;
    Ok(())
}

/// Write a projection for the next run, recording what it was computed from.
pub fn write_matrix(path: &Path, source: &str, values: &[f64], width: usize) -> Result<()> {
    let n_rows = values.len().checked_div(width).unwrap_or(0);
    let pairs = (0..width)
        .map(|index| {
            let column: Vec<f64> = (0..n_rows).map(|row| values[row * width + index]).collect();
            (format!("dim_{index}"), column)
        })
        .collect();
    let table = Table::from_f64_columns(pairs)?;
    write_parquet_with_metadata(
        &table,
        path,
        &[(SOURCE_KEY.to_string(), source.to_string())],
    )?;
    Ok(())
}

/// A cached partition, on the same terms as [`fn@read_matrix`].
pub fn read_labels(path: &Path, source: &str, expected_rows: usize) -> Option<Vec<u32>> {
    if !path.is_file() || !came_from(path, source) {
        return None;
    }
    let table = read_table(path, Extension::Parquet).ok()?;
    if table.n_rows() != expected_rows {
        return None;
    }
    let labels = table.f64_column("niches").ok()?;
    Some(labels.into_iter().map(|v| v as u32).collect())
}

/// Write a partition for the next run, recording what it was computed from.
pub fn write_labels(path: &Path, source: &str, labels: &[u32]) -> Result<()> {
    let table = Table::from_columns(vec![(
        "niches".to_string(),
        Table::u32_array(labels.iter().copied()),
    )])?;
    write_parquet_with_metadata(
        &table,
        path,
        &[(SOURCE_KEY.to_string(), source.to_string())],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(var_aggreg: u32, reduction: u32, clustering: u32) -> RunNumbers {
        RunNumbers {
            var_aggreg,
            reduction,
            clustering,
        }
    }

    /// A partition sits beside the projection it came from, inside the
    /// aggregation that projection came from.
    #[test]
    fn the_files_nest_the_way_the_stages_do() {
        let caches = Caches::under(Path::new("/work"));
        let run = numbers(3, 2, 7);
        assert_eq!(
            caches.features_path(&run),
            Path::new("/work/temp/var_aggreg/3/var_aggreg_3.parquet")
        );
        assert_eq!(
            caches.reduction_path(&run),
            Path::new("/work/temp/var_aggreg/3/2/reduction_2.parquet")
        );
        assert_eq!(
            caches.clustering_path(&run),
            Path::new("/work/temp/var_aggreg/3/2/clustering_7.parquet")
        );
    }

    #[test]
    fn recorded_paths_are_relative_to_the_working_directory() {
        let caches = Caches::under(Path::new("/work"));
        assert_eq!(
            caches.relative(&caches.reduction_path(&numbers(3, 2, 7))),
            "temp/var_aggreg/3/2/reduction_2.parquet"
        );
    }

    #[test]
    fn a_matrix_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("red.parquet");
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        write_matrix(&path, "features", &values, 2).unwrap();

        let (back, width) = read_matrix(&path, "features", 3).unwrap();
        assert_eq!(width, 2);
        assert_eq!(back, values);
    }

    #[test]
    fn labels_survive_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clu.parquet");
        write_labels(&path, "projection", &[0, 1, 1, 2]).unwrap();
        assert_eq!(
            read_labels(&path, "projection", 4).unwrap(),
            vec![0, 1, 1, 2]
        );
    }

    /// The reason the source is recorded at all. The names carry only their own
    /// stage's settings, so two runs that project different feature tables with
    /// the same UMAP settings arrive at the same file name — and must not read
    /// each other's coordinates.
    #[test]
    fn a_cache_from_another_source_is_not_reused() {
        let dir = tempfile::tempdir().unwrap();

        let matrix = dir.path().join("red.parquet");
        write_matrix(
            &matrix,
            "var_mean_std_Cluster_order1_16pheno",
            &[1.0, 2.0],
            1,
        )
        .unwrap();
        assert!(read_matrix(&matrix, "var_mean_std_Cluster_order1_16pheno", 2).is_some());
        assert!(
            read_matrix(&matrix, "var_mean_std_CellType_order1_16pheno", 2).is_none(),
            "a projection of other features was reused"
        );

        let labels = dir.path().join("clu.parquet");
        write_labels(
            &labels,
            "red_umap_dim2_nneigh20_euclidean_mindist00",
            &[0, 1],
        )
        .unwrap();
        assert!(read_labels(&labels, "red_umap_dim2_nneigh20_euclidean_mindist00", 2).is_some());
        assert!(read_labels(&labels, "red_none", 2).is_none());
    }

    /// A file written without a source is not one of ours — a half-finished
    /// write, or a parquet from another version — and guessing is exactly the
    /// mistake this mechanism exists to prevent.
    #[test]
    fn a_file_without_a_recorded_source_is_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("red.parquet");
        let table = Table::from_f64_columns(vec![("dim_0".to_string(), vec![1.0, 2.0])]).unwrap();
        mosna_io::write::write_parquet::write_parquet(&table, &path).unwrap();

        assert!(read_matrix(&path, "anything", 2).is_none());
    }

    /// The point of the height check: a cohort that grew must not read back the
    /// projection of the cohort it used to be.
    #[test]
    fn a_cache_of_the_wrong_height_is_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let matrix = dir.path().join("red.parquet");
        write_matrix(&matrix, "features", &[1.0, 2.0, 3.0, 4.0], 2).unwrap();
        assert!(read_matrix(&matrix, "features", 2).is_some());
        assert!(read_matrix(&matrix, "features", 3).is_none());

        let labels = dir.path().join("clu.parquet");
        write_labels(&labels, "projection", &[0, 1]).unwrap();
        assert!(read_labels(&labels, "projection", 2).is_some());
        assert!(read_labels(&labels, "projection", 5).is_none());
    }

    #[test]
    fn an_absent_cache_is_simply_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_matrix(&dir.path().join("nothing.parquet"), "x", 10).is_none());
        assert!(read_labels(&dir.path().join("nothing.parquet"), "x", 10).is_none());
    }

    /// A file that is not a cache of ours — a truncated write, a parquet from
    /// another tool — must be recomputed, not reported as a failure.
    #[test]
    fn an_unreadable_cache_is_a_miss_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("red.parquet");
        std::fs::write(&path, b"not parquet at all").unwrap();
        assert!(read_matrix(&path, "x", 3).is_none());
        assert!(read_labels(&path, "x", 3).is_none());
    }
}
