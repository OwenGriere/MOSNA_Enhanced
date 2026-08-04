//! Port of `mosna/preprocessing.py::make_data_index`.

use std::path::Path;

use crate::discovery::find_sample::find_sample;
use crate::discovery::find_sample_from_file::find_sample_from_file;
use crate::discovery::sample_id::SampleId;
use crate::error::Result;

/// Build the index of `(patient, sample)` identifiers present in a directory.
///
/// # Ordering
///
/// The Python original iterates `nodes_dir.glob(...)`, which yields entries in
/// filesystem order — arbitrary, and different between two machines holding the
/// same data. Every consumer of that index nevertheless assumes it lines up
/// with data gathered elsewhere in sorted order: `merge_niche_pheno` walks
/// `sorted(net_dir.glob("nodes_*.parquet"))` and `aggregated_niches` builds its
/// cohort table from `find_sample`, which sorts.
///
/// This port sorts, so the aggregated feature table, the cell-type vector and
/// the per-file niche labels are all in one order. That is a deliberate
/// difference from Python: it makes runs reproducible and removes a real
/// mismatch risk. Results are unaffected whenever the Python glob happened to
/// return sorted names, which is the common case on ext4 for these short,
/// uniform file names.
pub fn make_data_index(
    nodes_dir: impl AsRef<Path>,
    patient_column: &str,
    sample_column: Option<&str>,
    extension: &str,
) -> Result<Vec<SampleId>> {
    let files = find_sample(nodes_dir, extension, patient_column, sample_column)?;
    files
        .iter()
        .map(|file| find_sample_from_file(file, patient_column, sample_column))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"").unwrap();
    }

    #[test]
    fn indexes_two_level_data_in_sorted_order() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "nodes_patient-2_sample-1.parquet");
        touch(p, "nodes_patient-1_sample-2.parquet");
        touch(p, "edges_patient-1_sample-2.parquet");

        let index = make_data_index(p, "patient", Some("sample"), "parquet").unwrap();
        assert_eq!(
            index,
            vec![
                SampleId::with_sample("1", "2"),
                SampleId::with_sample("2", "1"),
            ]
        );
    }

    #[test]
    fn indexes_single_level_data() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "nodes_patient-5.parquet");

        let index = make_data_index(p, "patient", None, "parquet").unwrap();
        assert_eq!(index, vec![SampleId::patient_only("5")]);
    }

    #[test]
    fn ignores_files_that_are_not_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "nodes_patient-1.parquet");
        touch(p, "var_aggreg.parquet");
        touch(p, "cell_types.npy");

        assert_eq!(
            make_data_index(p, "patient", None, "parquet")
                .unwrap()
                .len(),
            1
        );
    }
}
