//! Synthetic datasets whose structure is known in advance.
//!
//! Clustering and embedding tests need inputs whose right answer is not in
//! doubt: if `blobs` produces three groups separated by ten units and a
//! clusterer asked for three clusters splits one of them, the clusterer is
//! wrong, not the test.

use std::path::{Path, PathBuf};

use mosna_io::write::write_parquet::write_parquet;
use mosna_io::Table;

/// `k` compact, well-separated Gaussian-ish blobs in the plane.
///
/// Returns the points row-major (`n_per_blob * k` rows of 2) and the blob index
/// each point came from, so a test can compare a clustering against the truth.
/// The layout is deterministic — points are placed on a small circle around
/// each centre rather than drawn randomly — so a failure is always replayable.
pub fn blobs(k: usize, n_per_blob: usize, separation: f64) -> (Vec<f64>, Vec<u32>) {
    let mut points = Vec::with_capacity(k * n_per_blob * 2);
    let mut truth = Vec::with_capacity(k * n_per_blob);

    for blob in 0..k {
        let angle = blob as f64 * std::f64::consts::TAU / k.max(1) as f64;
        let (cx, cy) = (separation * angle.cos(), separation * angle.sin());
        for i in 0..n_per_blob {
            let t = i as f64 * std::f64::consts::TAU / n_per_blob.max(1) as f64;
            // Radius well under `separation` so the blobs cannot touch.
            points.push(cx + 0.3 * t.cos());
            points.push(cy + 0.3 * t.sin());
            truth.push(blob as u32);
        }
    }
    (points, truth)
}

/// A `rows x cols` unit grid of points, row-major as `[x, y]` pairs.
///
/// The Delaunay triangulation of a grid is a useful stress case: it is full of
/// cocircular quadruples, so the triangulation is not unique and any code that
/// assumes a particular diagonal will show it.
pub fn grid(rows: usize, cols: usize) -> Vec<[f64; 2]> {
    let mut points = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            points.push([i as f64, j as f64]);
        }
    }
    points
}

/// `n` points evenly spaced on the unit circle.
///
/// Their nearest-neighbour graph is a cycle, whose properties — every degree
/// exactly two, one connected component — are easy to assert.
pub fn ring(n: usize) -> Vec<[f64; 2]> {
    (0..n)
        .map(|i| {
            let t = i as f64 * std::f64::consts::TAU / n.max(1) as f64;
            [t.cos(), t.sin()]
        })
        .collect()
}

/// A cohort of network files on disk, in the layout the pipelines expect.
pub struct Cohort {
    /// Kept alive so the directory outlives the test.
    _dir: tempfile::TempDir,
    pub net_dir: PathBuf,
    pub patient_column: String,
    pub sample_column: Option<String>,
    /// Phenotype vocabulary used across the cohort.
    pub phenotypes: Vec<String>,
}

impl Cohort {
    /// The directory holding the `nodes_*` / `edges_*` files.
    pub fn dir(&self) -> &Path {
        &self.net_dir
    }
}

/// Write a small two-level cohort of `n_samples` networks to a temporary
/// directory.
///
/// Each network is a path graph of `n_cells` cells whose phenotypes cycle
/// through `phenotypes`, so the abundances are known and the neighbourhood
/// composition of every cell is computable by hand.
pub fn cohort(n_samples: usize, n_cells: usize, phenotypes: &[&str]) -> Cohort {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let net_dir = dir.path().to_path_buf();

    for sample in 0..n_samples {
        let labels: Vec<&str> = (0..n_cells)
            .map(|i| phenotypes[i % phenotypes.len()])
            .collect();

        let nodes = Table::from_columns(vec![
            (
                "X_position".into(),
                Table::f64_array((0..n_cells).map(|i| i as f64)),
            ),
            (
                "Y_position".into(),
                Table::f64_array((0..n_cells).map(|i| (i % 3) as f64)),
            ),
            ("Cluster".into(), Table::string_array(labels)),
        ])
        .expect("a well-formed nodes table");

        let pairs: Vec<(u32, u32)> = (0..n_cells.saturating_sub(1))
            .map(|i| (i as u32, i as u32 + 1))
            .collect();
        let edges = Table::from_edges(&pairs).expect("a well-formed edges table");

        let stem = format!("patient-{}_sample-1", sample + 1);
        write_parquet(&nodes, net_dir.join(format!("nodes_{stem}.parquet")))
            .expect("nodes must be writable");
        write_parquet(&edges, net_dir.join(format!("edges_{stem}.parquet")))
            .expect("edges must be writable");
    }

    Cohort {
        _dir: dir,
        net_dir,
        patient_column: "patient".into(),
        sample_column: Some("sample".into()),
        phenotypes: phenotypes.iter().map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobs_are_separated_by_more_than_their_radius() {
        let (points, truth) = blobs(3, 10, 10.0);
        assert_eq!(points.len(), 60);
        assert_eq!(truth.len(), 30);

        // Two points from different blobs are far apart; two from the same blob
        // are close.
        let distance = |a: usize, b: usize| {
            let dx = points[a * 2] - points[b * 2];
            let dy = points[a * 2 + 1] - points[b * 2 + 1];
            (dx * dx + dy * dy).sqrt()
        };
        assert!(distance(0, 1) < 1.0, "same blob");
        assert!(distance(0, 10) > 5.0, "different blobs");
    }

    #[test]
    fn the_grid_has_the_requested_shape() {
        let points = grid(3, 4);
        assert_eq!(points.len(), 12);
        assert_eq!(points[0], [0.0, 0.0]);
        assert_eq!(points[11], [2.0, 3.0]);
    }

    #[test]
    fn ring_points_lie_on_the_unit_circle() {
        for point in ring(16) {
            let radius = (point[0] * point[0] + point[1] * point[1]).sqrt();
            assert!((radius - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn the_cohort_is_discoverable_by_the_io_layer() {
        let cohort = cohort(3, 12, &["A", "B"]);
        let files =
            mosna_io::find_sample(cohort.dir(), "parquet", "patient", Some("sample")).unwrap();
        assert_eq!(files.len(), 3);

        let nodes = mosna_io::read::read_parquet::read_parquet(&files[0]).unwrap();
        assert_eq!(nodes.n_rows(), 12);
        assert!(nodes.has_column("Cluster"));
    }

    #[test]
    fn cohort_edges_form_a_path() {
        let cohort = cohort(1, 5, &["A"]);
        let edges = mosna_io::read::read_parquet::read_parquet(
            cohort.dir().join("edges_patient-1_sample-1.parquet"),
        )
        .unwrap();
        assert_eq!(edges.edges().unwrap(), vec![(0, 1), (1, 2), (2, 3), (3, 4)]);
    }
}
