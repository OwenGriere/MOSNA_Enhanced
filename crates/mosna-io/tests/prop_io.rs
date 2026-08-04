//! Property tests for the I/O layer.
//!
//! File naming is the contract between the three analysis steps: step 1 writes
//! `nodes_patient-X_sample-Y.parquet`, steps 2 and 3 find them again by
//! pattern and decode the identifiers back. A round-trip failure there loses
//! samples silently, so the encode/decode pair is the main thing pinned here.

use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::write::{write_csv::write_csv, write_parquet::write_parquet};
use mosna_io::{find_sample, find_sample_from_file, make_data_index, SampleId, Table};
use proptest::prelude::*;

/// Identifiers as the naming scheme allows them: no underscore, no dot, not
/// empty — `[^_]+` in the discovery regex.
fn identifier() -> impl Strategy<Value = String> {
    "[A-Za-z0-9][A-Za-z0-9-]{0,7}"
}

/// A column name, which may be any ordinary word.
fn column_name() -> impl Strategy<Value = String> {
    "[a-z][a-z_]{0,9}"
}

proptest! {
    /// Encoding an identifier into a file name and decoding it back is the
    /// identity, for both dataset shapes.
    ///
    /// No assumption is placed on the column names or the identifiers: the
    /// decoder matches the structure the encoder emits, so the round trip holds
    /// even when a column name collides with the `nodes_` prefix or with the
    /// other column's separator. Both of those cases were found by this test
    /// and silently returned a wrong patient id before the decoder was fixed.
    #[test]
    fn prop_sample_id_round_trips_through_the_file_name(
        patient in identifier(),
        sample in identifier(),
        patient_column in column_name(),
        sample_column in column_name(),
    ) {
        let two_level = SampleId::with_sample(&patient, &sample);
        let name = two_level.nodes_file_name(&patient_column, Some(&sample_column), "parquet");
        let decoded = find_sample_from_file(&name, &patient_column, Some(&sample_column)).unwrap();
        prop_assert_eq!(decoded, two_level);

        let one_level = SampleId::patient_only(&patient);
        let name = one_level.nodes_file_name(&patient_column, None, "parquet");
        let decoded = find_sample_from_file(&name, &patient_column, None).unwrap();
        prop_assert_eq!(decoded, one_level);
    }

    /// A nodes file and its edges file name the same sample.
    #[test]
    fn prop_nodes_and_edges_names_agree(patient in identifier(), sample in identifier()) {
        let id = SampleId::with_sample(&patient, &sample);
        let nodes = id.nodes_file_name("patient", Some("sample"), "parquet");
        let edges = id.edges_file_name("patient", Some("sample"), "parquet");
        prop_assert_eq!(
            nodes.replacen("nodes_", "edges_", 1),
            edges
        );
    }

    /// Every file written under the convention is discovered again, and its
    /// identifier decodes back to the one it was written for.
    #[test]
    fn prop_written_cohorts_are_fully_discovered(
        ids in proptest::collection::hash_set(identifier(), 1..6),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let ids: Vec<String> = ids.into_iter().collect();

        for patient in &ids {
            let table = Table::from_columns(vec![("x".into(), Table::f64_array([1.0]))]).unwrap();
            let name = SampleId::patient_only(patient)
                .nodes_file_name("patient", None, "parquet");
            write_parquet(&table, dir.path().join(name)).unwrap();
        }

        let found = find_sample(dir.path(), "parquet", "patient", None).unwrap();
        prop_assert_eq!(found.len(), ids.len());

        let index = make_data_index(dir.path(), "patient", None, "parquet").unwrap();
        let mut decoded: Vec<String> = index.into_iter().map(|id| id.patient).collect();
        let mut expected = ids;
        decoded.sort();
        expected.sort();
        prop_assert_eq!(decoded, expected);
    }

    /// Discovery is sorted, so two machines holding the same data process the
    /// samples in the same order.
    #[test]
    fn prop_discovery_is_sorted(ids in proptest::collection::hash_set(identifier(), 1..8)) {
        let dir = tempfile::tempdir().unwrap();
        for patient in &ids {
            let table = Table::from_columns(vec![("x".into(), Table::f64_array([0.0]))]).unwrap();
            let name = format!("nodes_patient-{patient}.parquet");
            write_parquet(&table, dir.path().join(name)).unwrap();
        }

        let found = find_sample(dir.path(), "parquet", "patient", None).unwrap();
        let mut sorted = found.clone();
        sorted.sort();
        prop_assert_eq!(found, sorted);
    }

    /// A float column survives a parquet round-trip bit for bit.
    #[test]
    fn prop_parquet_round_trips_floats(
        column in proptest::collection::vec(-1.0e6f64..1.0e6, 1..50),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.parquet");

        let table = Table::from_columns(vec![
            ("value".into(), Table::f64_array(column.clone())),
        ])
        .unwrap();
        write_parquet(&table, &path).unwrap();

        let loaded = read_table(&path, Extension::Parquet).unwrap();
        prop_assert_eq!(loaded.f64_column("value").unwrap(), column);
    }

    /// A label column survives a parquet round-trip.
    #[test]
    fn prop_parquet_round_trips_labels(
        labels in proptest::collection::vec("[A-Za-z ]{1,12}", 1..40),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.parquet");

        let table = Table::from_columns(vec![
            ("Cluster".into(), Table::string_array(labels.iter())),
        ])
        .unwrap();
        write_parquet(&table, &path).unwrap();

        let loaded = read_table(&path, Extension::Parquet).unwrap();
        prop_assert_eq!(loaded.string_column("Cluster").unwrap(), labels);
    }

    /// A CSV round-trip preserves float values to full precision, because the
    /// writer emits the shortest round-tripping representation.
    #[test]
    fn prop_csv_round_trips_floats(
        column in proptest::collection::vec(-1.0e6f64..1.0e6, 1..30),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.csv");

        let table = Table::from_columns(vec![
            ("value".into(), Table::f64_array(column.clone())),
        ])
        .unwrap();
        write_csv(&table, &path).unwrap();

        let loaded = read_table(&path, Extension::Csv).unwrap();
        prop_assert_eq!(loaded.f64_column("value").unwrap(), column);
    }

    /// An edge list survives the round-trip through the parquet edges file.
    #[test]
    fn prop_edges_round_trip(
        raw in proptest::collection::vec((0u32..500, 0u32..500), 1..40),
    ) {
        let pairs: Vec<(u32, u32)> = raw;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edges.parquet");

        write_parquet(&Table::from_edges(&pairs).unwrap(), &path).unwrap();
        let loaded = read_table(&path, Extension::Parquet).unwrap();
        prop_assert_eq!(loaded.edges().unwrap(), pairs);
    }

    /// Adding a column leaves every other column untouched, which is what
    /// `merge_niche_pheno` relies on when it writes niche labels back into the
    /// nodes files.
    #[test]
    fn prop_set_column_preserves_the_others(
        xs in proptest::collection::vec(-100.0f64..100.0, 1..30),
    ) {
        let n = xs.len();
        let mut table = Table::from_columns(vec![
            ("x".into(), Table::f64_array(xs.clone())),
            ("label".into(), Table::string_array((0..n).map(|i| format!("c{i}")))),
        ])
        .unwrap();

        table
            .set_column("niches", Table::u32_array((0..n as u32).map(|i| i % 3)))
            .unwrap();

        prop_assert_eq!(table.f64_column("x").unwrap(), xs);
        prop_assert_eq!(table.n_rows(), n);
        prop_assert_eq!(table.column_names(), vec!["x", "label", "niches"]);
    }
}
