//! Port of `mosna/niches.py::aggregate_cell_types`.

use std::path::Path;

use mosna_io::read::get_opener::{read_table_columns, Extension};
use mosna_io::SampleId;

use crate::error::Result;

/// Gather the cell types of a cohort in the order the NAS feature table uses.
///
/// The niche labels come out of the clustering as one label per row of the
/// feature table; to describe what each niche is made of, the phenotype of the
/// matching cell is needed. This produces that vector, aligned cell for cell.
///
/// # A simpler route than the Python's
///
/// `aggregate_cell_types` reconstructs the order by taking the distinct
/// `(patient, sample)` pairs out of the feature table and then filtering a
/// pooled cohort dataframe for each one:
///
/// ```python
/// cell_types = cohort_data.loc[
///     (cohort_data[patient_col] == patient_id) &
///     (cohort_data[sample_col] == int(sample_id)), pheno_col]
/// ```
///
/// That is quadratic in the cohort size, and the `int(sample_id)` cast makes it
/// fail outright on any sample identifier that is not a number — `chunk-A` in a
/// perfectly ordinary dataset would raise.
///
/// The feature table is built by concatenating per-sample blocks in
/// `data_index` order, each block in nodes-file row order. So reading the
/// phenotype column of each file in that same order reproduces the alignment
/// exactly, in one linear pass, with no constraint on the identifiers.
pub fn aggregate_cell_types(
    net_dir: impl AsRef<Path>,
    data_index: &[SampleId],
    patient_column: &str,
    sample_column: Option<&str>,
    extension: Extension,
    phenotype_column: &str,
) -> Result<Vec<String>> {
    let net_dir = net_dir.as_ref();
    let mut cell_types = Vec::new();

    for id in data_index {
        let path =
            net_dir.join(id.nodes_file_name(patient_column, sample_column, extension.as_str()));
        let table = read_table_columns(&path, extension, &[phenotype_column])?;
        cell_types.extend(table.string_column(phenotype_column)?);
    }

    Ok(cell_types)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_io::write::write_parquet::write_parquet;
    use mosna_io::Table;

    fn write_sample(dir: &Path, patient: &str, sample: &str, phenotypes: &[&str]) {
        let table = Table::from_columns(vec![(
            "Cluster".into(),
            Table::string_array(phenotypes.iter()),
        )])
        .unwrap();
        write_parquet(
            &table,
            dir.join(format!("nodes_patient-{patient}_sample-{sample}.parquet")),
        )
        .unwrap();
    }

    #[test]
    fn concatenates_samples_in_index_order() {
        let dir = tempfile::tempdir().unwrap();
        write_sample(dir.path(), "1", "1", &["A", "B"]);
        write_sample(dir.path(), "2", "1", &["C"]);

        let index = vec![
            SampleId::with_sample("1", "1"),
            SampleId::with_sample("2", "1"),
        ];
        let cell_types = aggregate_cell_types(
            dir.path(),
            &index,
            "patient",
            Some("sample"),
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert_eq!(cell_types, vec!["A", "B", "C"]);
    }

    #[test]
    fn a_different_index_order_gives_a_different_concatenation() {
        let dir = tempfile::tempdir().unwrap();
        write_sample(dir.path(), "1", "1", &["A"]);
        write_sample(dir.path(), "2", "1", &["B"]);

        let reversed = vec![
            SampleId::with_sample("2", "1"),
            SampleId::with_sample("1", "1"),
        ];
        let cell_types = aggregate_cell_types(
            dir.path(),
            &reversed,
            "patient",
            Some("sample"),
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert_eq!(cell_types, vec!["B", "A"]);
    }

    /// The Python casts the sample id with `int(...)`; a textual identifier is
    /// perfectly legal in the file naming and must not break the join.
    #[test]
    fn non_numeric_sample_identifiers_are_supported() {
        let dir = tempfile::tempdir().unwrap();
        write_sample(dir.path(), "P1", "chunkA", &["A", "B"]);

        let cell_types = aggregate_cell_types(
            dir.path(),
            &[SampleId::with_sample("P1", "chunkA")],
            "patient",
            Some("sample"),
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert_eq!(cell_types, vec!["A", "B"]);
    }

    #[test]
    fn an_empty_index_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cell_types = aggregate_cell_types(
            dir.path(),
            &[],
            "patient",
            Some("sample"),
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert!(cell_types.is_empty());
    }
}
