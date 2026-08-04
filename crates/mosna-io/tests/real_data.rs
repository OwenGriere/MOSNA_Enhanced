//! Exercises discovery and reading against the datasets checked into `test/`.
//!
//! These are the same files the Python application is run on during manual
//! testing, so they pin the port to real inputs rather than to synthetic ones.

use std::path::PathBuf;

use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::{find_sample, find_sample_from_file, make_data_index, SampleId};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `test/patient_folder` holds three single-level CSV datasets.
#[test]
fn single_level_csv_dataset() {
    let dir = repo_root().join("test/patient_folder");
    if !dir.is_dir() {
        eprintln!("skipping: {} not found", dir.display());
        return;
    }

    let files = find_sample(&dir, "csv", "patient", None).unwrap();
    assert_eq!(files.len(), 3, "expected three nodes files");

    let index = make_data_index(&dir, "patient", None, "csv").unwrap();
    assert_eq!(
        index,
        vec![
            SampleId::patient_only("01"),
            SampleId::patient_only("02"),
            SampleId::patient_only("03"),
        ]
    );

    let table = read_table(&files[0], Extension::Csv).unwrap();
    assert_eq!(table.column_names(), vec!["id", "X", "Y", "phenotype"]);
    assert!(table.n_rows() > 0);

    // Coordinates must come back as finite floats, and phenotypes as labels.
    let coords = table.coords("X", "Y").unwrap();
    assert!(coords.iter().all(|[x, y]| x.is_finite() && y.is_finite()));
    let phenotypes = table.dropna_string_column("phenotype").unwrap();
    assert_eq!(phenotypes.len(), table.n_rows());
    assert!(phenotypes.iter().any(|p| p == "cancer"));
}

/// `test/patient_sample_folder` holds two-level parquet datasets whose sample
/// level is named `chunk`, which checks the column names are not hard-coded.
#[test]
fn two_level_parquet_dataset() {
    let dir = repo_root().join("test/patient_sample_folder");
    if !dir.is_dir() {
        eprintln!("skipping: {} not found", dir.display());
        return;
    }

    let files = find_sample(&dir, "parquet", "patient", Some("chunk")).unwrap();
    assert!(!files.is_empty(), "expected parquet nodes files");

    for file in &files {
        let id = find_sample_from_file(file, "patient", Some("chunk")).unwrap();
        assert!(id.sample.is_some(), "{file:?} must decode a chunk id");
        // The decoded identifier must rebuild the file name it came from.
        assert_eq!(
            id.nodes_file_name("patient", Some("chunk"), "parquet"),
            file.file_name().unwrap().to_string_lossy()
        );
    }

    let table = read_table(&files[0], Extension::Parquet).unwrap();
    assert!(table.n_rows() > 0);
    assert!(table.n_columns() > 0);
    println!(
        "{}: {} rows x {} columns {:?}",
        files[0].display(),
        table.n_rows(),
        table.n_columns(),
        table.column_names()
    );
}

/// A single-level pattern must not pick up the two-level files, and vice versa.
#[test]
fn patterns_do_not_leak_between_datasets() {
    let dir = repo_root().join("test/patient_sample_folder");
    if !dir.is_dir() {
        return;
    }
    assert!(
        find_sample(&dir, "parquet", "patient", None)
            .unwrap()
            .is_empty(),
        "single-level pattern must not match two-level files"
    );
}
