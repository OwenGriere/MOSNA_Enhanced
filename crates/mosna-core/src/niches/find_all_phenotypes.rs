//! Port of `package/core/NAS/find_all_pheno.py::find_all_pheno`.

use std::path::Path;

use mosna_io::read::get_opener::{read_table_columns, Extension};
use mosna_io::SampleId;

use crate::error::Result;

/// Collect the phenotype vocabulary of a whole cohort.
///
/// This list fixes the column order of the NAS feature table: every sample is
/// one-hot encoded against it, so a sample missing a phenotype still produces a
/// column of zeros in the right place. Getting a different order for two
/// samples would silently mix up the features.
///
/// # Ordering
///
/// The Python returns `df_tot[pheno_col].unique().tolist()`, which is
/// *first-seen* order over a concatenation of the files in glob order — so it
/// depends on the filesystem. This sorts instead, which makes the feature
/// columns reproducible across machines. The set is identical either way, and
/// nothing downstream depends on the order beyond internal consistency.
pub fn find_all_phenotypes(
    net_dir: impl AsRef<Path>,
    data_index: &[SampleId],
    patient_column: &str,
    sample_column: Option<&str>,
    extension: Extension,
    phenotype_column: &str,
) -> Result<Vec<String>> {
    let net_dir = net_dir.as_ref();
    let mut phenotypes = std::collections::BTreeSet::new();

    for id in data_index {
        let path =
            net_dir.join(id.nodes_file_name(patient_column, sample_column, extension.as_str()));
        // Only the phenotype column is decoded; for parquet the projection is
        // pushed into the reader, so the rest of the file is never touched.
        let table = read_table_columns(&path, extension, &[phenotype_column])?;
        phenotypes.extend(table.dropna_string_column(phenotype_column)?);
    }

    Ok(phenotypes.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_io::write::write_parquet::write_parquet;
    use mosna_io::Table;

    fn write_sample(dir: &Path, patient: &str, phenotypes: &[&str]) {
        let table = Table::from_columns(vec![
            ("Cluster".into(), Table::string_array(phenotypes.iter())),
            (
                "x".into(),
                Table::f64_array((0..phenotypes.len()).map(|i| i as f64)),
            ),
        ])
        .unwrap();
        write_parquet(&table, dir.join(format!("nodes_patient-{patient}.parquet"))).unwrap();
    }

    #[test]
    fn gathers_the_union_across_samples() {
        let dir = tempfile::tempdir().unwrap();
        write_sample(dir.path(), "1", &["A", "B"]);
        write_sample(dir.path(), "2", &["B", "C"]);

        let index = vec![SampleId::patient_only("1"), SampleId::patient_only("2")];
        let phenotypes = find_all_phenotypes(
            dir.path(),
            &index,
            "patient",
            None,
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert_eq!(phenotypes, vec!["A", "B", "C"]);
    }

    #[test]
    fn the_order_does_not_depend_on_the_file_order() {
        let dir = tempfile::tempdir().unwrap();
        write_sample(dir.path(), "1", &["Z"]);
        write_sample(dir.path(), "2", &["A"]);

        let forward = find_all_phenotypes(
            dir.path(),
            &[SampleId::patient_only("1"), SampleId::patient_only("2")],
            "patient",
            None,
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        let backward = find_all_phenotypes(
            dir.path(),
            &[SampleId::patient_only("2"), SampleId::patient_only("1")],
            "patient",
            None,
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert_eq!(forward, backward);
        assert_eq!(forward, vec!["A", "Z"]);
    }

    #[test]
    fn nulls_are_not_part_of_the_vocabulary() {
        let dir = tempfile::tempdir().unwrap();
        let array: arrow_array::ArrayRef =
            std::sync::Arc::new(arrow_array::StringArray::from(vec![
                Some("A"),
                None,
                Some("B"),
            ]));
        let table = Table::from_columns(vec![("Cluster".into(), array)]).unwrap();
        write_parquet(&table, dir.path().join("nodes_patient-1.parquet")).unwrap();

        let phenotypes = find_all_phenotypes(
            dir.path(),
            &[SampleId::patient_only("1")],
            "patient",
            None,
            Extension::Parquet,
            "Cluster",
        )
        .unwrap();
        assert_eq!(phenotypes, vec!["A", "B"]);
    }

    #[test]
    fn a_missing_column_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write_sample(dir.path(), "1", &["A"]);
        let err = find_all_phenotypes(
            dir.path(),
            &[SampleId::patient_only("1")],
            "patient",
            None,
            Extension::Parquet,
            "Phenotype",
        )
        .unwrap_err();
        assert!(err.to_string().contains("Phenotype"));
    }
}
