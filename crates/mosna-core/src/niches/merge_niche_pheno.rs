//! Port of `package/core/NAS/merge_niche_pheno.py::merge_niche_pheno`.

use std::path::Path;

use mosna_io::read::get_opener::{read_table, read_table_columns, Extension};
use mosna_io::write::write_parquet::write_parquet;
use mosna_io::{SampleId, Table};

use crate::error::{CoreError, Result};

/// Write the niche label of every cell back into its nodes file.
///
/// `niches` is one label per cell, in the order the NAS feature table used —
/// the samples of `data_index` concatenated, each in file order. The vector is
/// split back across the samples and written as a `niches` column, which is
/// what lets the network re-plot colour each cell by its niche.
///
/// The total length is checked against the cohort before anything is written:
/// a mismatch means the labels would be attributed to the wrong cells, and
/// nothing downstream could detect that.
pub fn merge_niche_pheno(
    net_dir: impl AsRef<Path>,
    data_index: &[SampleId],
    patient_column: &str,
    sample_column: Option<&str>,
    extension: Extension,
    niches: &[u32],
) -> Result<()> {
    let net_dir = net_dir.as_ref();

    // First pass: how many cells each sample holds. Reading a single column
    // keeps this cheap even though it touches every file.
    let mut paths = Vec::with_capacity(data_index.len());
    let mut lengths = Vec::with_capacity(data_index.len());
    for id in data_index {
        let path =
            net_dir.join(id.nodes_file_name(patient_column, sample_column, extension.as_str()));
        let table = read_table(&path, extension)?;
        lengths.push(table.n_rows());
        paths.push(path);
    }

    let total: usize = lengths.iter().sum();
    if total != niches.len() {
        return Err(CoreError::shape(format!(
            "{} niche labels for {total} cells across {} sample(s)",
            niches.len(),
            data_index.len()
        )));
    }

    // Second pass: write each sample's slice back.
    let mut offset = 0usize;
    for (path, length) in paths.iter().zip(&lengths) {
        let mut table = read_table(path, extension)?;
        let slice = &niches[offset..offset + length];
        table.set_column("niches", Table::u32_array(slice.iter().copied()))?;
        // Always parquet: the network directory the pipelines write to is
        // parquet, and every later step reads it as such.
        write_parquet(&table, path)?;
        offset += length;
    }

    Ok(())
}

/// Number of cells in each sample, without decoding the whole file.
///
/// Used by the pipelines to size the niche vector before clustering.
pub fn sample_lengths(
    net_dir: impl AsRef<Path>,
    data_index: &[SampleId],
    patient_column: &str,
    sample_column: Option<&str>,
    extension: Extension,
    any_column: &str,
) -> Result<Vec<usize>> {
    let net_dir = net_dir.as_ref();
    data_index
        .iter()
        .map(|id| {
            let path =
                net_dir.join(id.nodes_file_name(patient_column, sample_column, extension.as_str()));
            Ok(read_table_columns(&path, extension, &[any_column])?.n_rows())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_testkit::fixtures::cohort;

    #[test]
    fn splits_the_labels_across_the_samples() {
        let fixture = cohort(2, 4, &["A", "B"]);
        let index = vec![
            SampleId::with_sample("1", "1"),
            SampleId::with_sample("2", "1"),
        ];
        let niches: Vec<u32> = vec![0, 0, 0, 0, 1, 1, 1, 1];

        merge_niche_pheno(
            fixture.dir(),
            &index,
            "patient",
            Some("sample"),
            Extension::Parquet,
            &niches,
        )
        .unwrap();

        for (position, id) in index.iter().enumerate() {
            let path = fixture
                .dir()
                .join(id.nodes_file_name("patient", Some("sample"), "parquet"));
            let table = read_table(&path, Extension::Parquet).unwrap();
            let written = table.f64_column("niches").unwrap();
            assert!(written.iter().all(|&v| v == position as f64));
        }
    }

    #[test]
    fn the_original_columns_survive() {
        let fixture = cohort(1, 5, &["A"]);
        let index = vec![SampleId::with_sample("1", "1")];
        merge_niche_pheno(
            fixture.dir(),
            &index,
            "patient",
            Some("sample"),
            Extension::Parquet,
            &[0, 1, 2, 3, 4],
        )
        .unwrap();

        let table = read_table(
            fixture.dir().join("nodes_patient-1_sample-1.parquet"),
            Extension::Parquet,
        )
        .unwrap();
        assert_eq!(
            table.column_names(),
            vec!["X_position", "Y_position", "Cluster", "niches"]
        );
    }

    #[test]
    fn a_length_mismatch_is_refused_before_writing() {
        let fixture = cohort(1, 5, &["A"]);
        let index = vec![SampleId::with_sample("1", "1")];

        let err = merge_niche_pheno(
            fixture.dir(),
            &index,
            "patient",
            Some("sample"),
            Extension::Parquet,
            &[0, 1],
        )
        .unwrap_err();
        assert!(err.to_string().contains('2') && err.to_string().contains('5'));

        // Nothing was written.
        let table = read_table(
            fixture.dir().join("nodes_patient-1_sample-1.parquet"),
            Extension::Parquet,
        )
        .unwrap();
        assert!(!table.has_column("niches"));
    }

    #[test]
    fn sample_lengths_reports_the_row_counts() {
        let fixture = cohort(3, 7, &["A"]);
        let index = vec![
            SampleId::with_sample("1", "1"),
            SampleId::with_sample("2", "1"),
            SampleId::with_sample("3", "1"),
        ];
        let lengths = sample_lengths(
            fixture.dir(),
            &index,
            "patient",
            Some("sample"),
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert_eq!(lengths, vec![7, 7, 7]);
    }
}
