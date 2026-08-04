//! Tests for niche composition and the niche/phenotype join.
//!
//! Written before the implementation. Unlike the clustering, these have exact
//! expected answers: a composition matrix is a count, and the alignment between
//! niche labels and cells is either right or it silently attributes a cell type
//! to the wrong niche.

use mosna_core::niches::{
    aggregate_cell_types, find_all_phenotypes, make_niches_composition, merge_niche_pheno,
    Normalize,
};
use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::{make_data_index, SampleId};
use mosna_testkit::fixtures::cohort;

fn labels(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------------------
// make_niches_composition
// ---------------------------------------------------------------------------

#[test]
fn composition_counts_cells_per_phenotype_and_niche() {
    // Six cells: A/A/B in niche 0, B/B/A in niche 1.
    let var = labels(&["A", "A", "B", "B", "B", "A"]);
    let niches = vec![0u32, 0, 0, 1, 1, 1];

    let composition = make_niches_composition(&var, &niches, Normalize::None).unwrap();

    assert_eq!(composition.phenotypes, labels(&["A", "B"]));
    assert_eq!(composition.niches, vec![0, 1]);
    // Row-major: A/niche0, A/niche1, B/niche0, B/niche1.
    assert_eq!(composition.counts, vec![2.0, 1.0, 1.0, 2.0]);
}

#[test]
fn phenotypes_and_niches_come_back_sorted() {
    let var = labels(&["Z", "A", "M"]);
    let niches = vec![5u32, 1, 3];
    let composition = make_niches_composition(&var, &niches, Normalize::None).unwrap();
    assert_eq!(composition.phenotypes, labels(&["A", "M", "Z"]));
    assert_eq!(composition.niches, vec![1, 3, 5]);
}

/// A phenotype absent from a niche must appear as an explicit zero, not be
/// missing: the matrix is rectangular and the figures index it by position.
#[test]
fn absent_combinations_are_zeros() {
    let var = labels(&["A", "B"]);
    let niches = vec![0u32, 1];
    let composition = make_niches_composition(&var, &niches, Normalize::None).unwrap();
    assert_eq!(composition.counts, vec![1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn total_normalisation_divides_by_the_cell_count() {
    let var = labels(&["A", "A", "B", "B"]);
    let niches = vec![0u32, 0, 1, 1];
    let composition = make_niches_composition(&var, &niches, Normalize::Total).unwrap();
    assert_eq!(composition.counts.iter().sum::<f64>(), 1.0);
    assert_eq!(composition.counts, vec![0.5, 0.0, 0.0, 0.5]);
}

/// `obs` normalises per phenotype: each row answers "where does this cell type
/// live?".
#[test]
fn obs_normalisation_makes_each_phenotype_sum_to_one() {
    let var = labels(&["A", "A", "A", "B"]);
    let niches = vec![0u32, 0, 1, 1];
    let composition = make_niches_composition(&var, &niches, Normalize::Obs).unwrap();

    let n_niches = composition.niches.len();
    for (row, phenotype) in composition.phenotypes.iter().enumerate() {
        let sum: f64 = composition.counts[row * n_niches..(row + 1) * n_niches]
            .iter()
            .sum();
        assert!((sum - 1.0).abs() < 1e-12, "{phenotype} sums to {sum}");
    }
    // A is two thirds in niche 0, one third in niche 1.
    assert!((composition.counts[0] - 2.0 / 3.0).abs() < 1e-12);
}

/// `niche` normalises per niche: each column answers "what is this niche made
/// of?".
#[test]
fn niche_normalisation_makes_each_niche_sum_to_one() {
    let var = labels(&["A", "B", "B", "A"]);
    let niches = vec![0u32, 0, 0, 1];
    let composition = make_niches_composition(&var, &niches, Normalize::Niche).unwrap();

    let n_niches = composition.niches.len();
    for column in 0..n_niches {
        let sum: f64 = (0..composition.phenotypes.len())
            .map(|row| composition.counts[row * n_niches + column])
            .sum();
        assert!((sum - 1.0).abs() < 1e-12, "niche {column} sums to {sum}");
    }
}

#[test]
fn clr_normalisation_stays_finite_despite_zeros() {
    let var = labels(&["A", "A", "B"]);
    let niches = vec![0u32, 0, 1];
    let composition = make_niches_composition(&var, &niches, Normalize::Clr).unwrap();
    assert!(
        composition.counts.iter().all(|v| v.is_finite()),
        "clr produced a non-finite value: {:?}",
        composition.counts
    );
}

#[test]
fn composition_rejects_mismatched_lengths() {
    let err = make_niches_composition(&labels(&["A"]), &[0, 1], Normalize::None).unwrap_err();
    assert!(err.to_string().contains("2"), "{err}");
}

#[test]
fn an_empty_composition_is_empty_not_an_error() {
    let composition = make_niches_composition(&[], &[], Normalize::Total).unwrap();
    assert!(composition.counts.is_empty());
    assert!(composition.phenotypes.is_empty());
}

// ---------------------------------------------------------------------------
// find_all_phenotypes
// ---------------------------------------------------------------------------

/// The phenotype vocabulary fixes the column order of the whole NAS feature
/// table, so it has to be the same for every sample and stable across runs.
#[test]
fn phenotypes_are_gathered_across_the_whole_cohort() {
    let fixture = cohort(3, 12, &["A", "B", "C"]);
    let index = make_data_index(fixture.dir(), "patient", Some("sample"), "parquet").unwrap();

    let phenotypes = find_all_phenotypes(
        fixture.dir(),
        &index,
        "patient",
        Some("sample"),
        Extension::Parquet,
        "Cluster",
    )
    .unwrap();

    assert_eq!(phenotypes, labels(&["A", "B", "C"]));
}

#[test]
fn the_phenotype_vocabulary_is_deduplicated_and_stable() {
    let fixture = cohort(2, 9, &["B", "A"]);
    let index = make_data_index(fixture.dir(), "patient", Some("sample"), "parquet").unwrap();

    let first = find_all_phenotypes(
        fixture.dir(),
        &index,
        "patient",
        Some("sample"),
        Extension::Parquet,
        "Cluster",
    )
    .unwrap();
    let second = find_all_phenotypes(
        fixture.dir(),
        &index,
        "patient",
        Some("sample"),
        Extension::Parquet,
        "Cluster",
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
}

// ---------------------------------------------------------------------------
// aggregate_cell_types
// ---------------------------------------------------------------------------

/// The cell-type vector must line up cell for cell with the rows of the NAS
/// feature table. A shift here silently attributes every cell type to the wrong
/// niche, and nothing downstream would notice.
#[test]
fn cell_types_are_gathered_in_feature_table_order() {
    let fixture = cohort(3, 10, &["A", "B"]);
    let index = make_data_index(fixture.dir(), "patient", Some("sample"), "parquet").unwrap();

    let cell_types = aggregate_cell_types(
        fixture.dir(),
        &index,
        "patient",
        Some("sample"),
        Extension::Parquet,
        "Cluster",
    )
    .unwrap();

    assert_eq!(cell_types.len(), 30, "three samples of ten cells");

    // Rebuild the expected order by reading the files in index order.
    let mut expected = Vec::new();
    for id in &index {
        let path = fixture
            .dir()
            .join(id.nodes_file_name("patient", Some("sample"), "parquet"));
        let table = read_table(&path, Extension::Parquet).unwrap();
        expected.extend(table.string_column("Cluster").unwrap());
    }
    assert_eq!(cell_types, expected);
}

// ---------------------------------------------------------------------------
// merge_niche_pheno
// ---------------------------------------------------------------------------

/// Niche labels are written back into each nodes file, split across the samples
/// in the same order the features were computed in.
#[test]
fn niche_labels_are_written_back_to_each_sample() {
    let fixture = cohort(3, 10, &["A", "B"]);
    let index = make_data_index(fixture.dir(), "patient", Some("sample"), "parquet").unwrap();

    // Sample 0 all niche 0, sample 1 all niche 1, sample 2 all niche 2.
    let niches: Vec<u32> = (0..30).map(|i| (i / 10) as u32).collect();

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

        assert!(table.has_column("niches"), "{path:?} has no niches column");
        let written = table.f64_column("niches").unwrap();
        assert_eq!(written.len(), 10);
        assert!(
            written.iter().all(|&v| v == position as f64),
            "sample {position} got {written:?}"
        );
        // The original columns must survive untouched.
        assert!(table.has_column("Cluster"));
        assert!(table.has_column("X_position"));
    }
}

#[test]
fn merging_is_idempotent() {
    let fixture = cohort(2, 6, &["A"]);
    let index = make_data_index(fixture.dir(), "patient", Some("sample"), "parquet").unwrap();
    let niches: Vec<u32> = (0..12).map(|i| (i % 3) as u32).collect();

    for _ in 0..2 {
        merge_niche_pheno(
            fixture.dir(),
            &index,
            "patient",
            Some("sample"),
            Extension::Parquet,
            &niches,
        )
        .unwrap();
    }

    let path = fixture
        .dir()
        .join(index[0].nodes_file_name("patient", Some("sample"), "parquet"));
    let table = read_table(&path, Extension::Parquet).unwrap();
    // Re-running must not append a second niches column.
    assert_eq!(
        table
            .column_names()
            .iter()
            .filter(|n| **n == "niches")
            .count(),
        1
    );
}

/// A niche vector that does not cover the cohort exactly is a programming
/// error, and one that would otherwise assign labels to the wrong cells.
#[test]
fn merging_rejects_a_length_mismatch() {
    let fixture = cohort(2, 5, &["A"]);
    let index = make_data_index(fixture.dir(), "patient", Some("sample"), "parquet").unwrap();

    let err = merge_niche_pheno(
        fixture.dir(),
        &index,
        "patient",
        Some("sample"),
        Extension::Parquet,
        &[0, 1, 2],
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains('3') && message.contains("10"), "{message}");
}

#[test]
fn a_single_level_cohort_is_handled() {
    let fixture = cohort(1, 8, &["A", "B"]);
    // Re-read with the two-level naming the fixture uses, then check the
    // single-level path is not accidentally required.
    let index = vec![SampleId::with_sample("1", "1")];

    let cell_types = aggregate_cell_types(
        fixture.dir(),
        &index,
        "patient",
        Some("sample"),
        Extension::Parquet,
        "Cluster",
    )
    .unwrap();
    assert_eq!(cell_types.len(), 8);
}
