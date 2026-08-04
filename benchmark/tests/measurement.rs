//! Tests of the timing harness and the fingerprints, written before the
//! implementation.
//!
//! Two things a benchmark must not get wrong: reporting a number that is not
//! the one it measured, and declaring two runs equal when they are not.

use std::time::Duration;

use mosna_bench::fingerprint::Fingerprint;
use mosna_bench::timing::Samples;

// ---------------------------------------------------------------------------
// Summarising timings
// ---------------------------------------------------------------------------

fn millis(values: &[u64]) -> Samples {
    Samples::from(
        values
            .iter()
            .map(|&v| Duration::from_millis(v))
            .collect::<Vec<_>>(),
    )
}

/// The median, not the mean: the first run pays for cold caches and page
/// faults, and an average lets that one run dominate the report.
#[test]
fn the_median_of_an_odd_count_is_the_middle_value() {
    assert_eq!(millis(&[30, 10, 20]).median(), Duration::from_millis(20));
}

#[test]
fn the_median_of_an_even_count_is_the_mean_of_the_two_middles() {
    assert_eq!(
        millis(&[10, 20, 30, 40]).median(),
        Duration::from_millis(25)
    );
}

/// A cold first run must not move the median.
#[test]
fn one_slow_outlier_does_not_move_the_median() {
    let with = millis(&[10, 11, 12, 13, 900]);
    let without = millis(&[10, 11, 12, 13, 14]);
    assert_eq!(with.median(), without.median());
}

/// The median absolute deviation says how much to trust the median. A spread
/// of zero means the machine was quiet; a large one means the number is noise.
#[test]
fn the_deviation_is_zero_for_identical_runs() {
    assert_eq!(millis(&[42, 42, 42]).mad(), Duration::ZERO);
}

#[test]
fn the_deviation_reports_the_typical_distance_from_the_median() {
    // Median 30; deviations 20, 10, 0, 10, 20; median of those is 10.
    assert_eq!(
        millis(&[10, 20, 30, 40, 50]).mad(),
        Duration::from_millis(10)
    );
}

#[test]
fn a_single_run_has_no_spread() {
    let one = millis(&[7]);
    assert_eq!(one.median(), Duration::from_millis(7));
    assert_eq!(one.mad(), Duration::ZERO);
}

#[test]
fn no_run_at_all_is_not_a_panic() {
    let none = millis(&[]);
    assert_eq!(none.median(), Duration::ZERO);
    assert!(none.is_empty());
}

/// The harness must run the closure exactly as many times as asked, and report
/// that many samples.
#[test]
fn measuring_runs_the_work_the_requested_number_of_times() {
    let mut calls = 0;
    let samples = mosna_bench::timing::measure(4, || calls += 1);
    assert_eq!(calls, 4);
    assert_eq!(samples.len(), 4);
}

// ---------------------------------------------------------------------------
// Fingerprints
// ---------------------------------------------------------------------------

fn reference() -> Fingerprint {
    let mut fingerprint = Fingerprint::default();
    fingerprint.pairs("edges", &[(0, 1), (1, 2), (2, 3)]);
    fingerprint.labels("niches", &[0, 0, 1, 1]);
    fingerprint.floats("assortativity", &[0.41, -0.12, 0.0]);
    fingerprint
}

#[test]
fn a_fingerprint_matches_itself() {
    assert!(reference().differences(&reference(), 1e-12).is_empty());
}

/// Edges come back from a parallel pipeline in whatever order the threads
/// finished in. That is not a difference, and reporting it as one would make
/// the whole level useless.
#[test]
fn the_order_of_the_edges_is_not_a_difference() {
    let mut shuffled = Fingerprint::default();
    shuffled.pairs("edges", &[(2, 3), (0, 1), (1, 2)]);
    shuffled.labels("niches", &[0, 0, 1, 1]);
    shuffled.floats("assortativity", &[0.41, -0.12, 0.0]);

    assert!(shuffled.differences(&reference(), 1e-12).is_empty());
}

/// A single changed edge is a changed graph, and must be reported.
#[test]
fn one_different_edge_is_reported() {
    let mut changed = reference();
    changed.pairs("edges", &[(0, 1), (1, 2), (2, 4)]);

    let differences = changed.differences(&reference(), 1e-12);
    assert_eq!(differences.len(), 1);
    assert!(differences[0].contains("edges"), "{differences:?}");
}

/// Labels are exact: a niche that moved is a niche that moved.
#[test]
fn one_relabelled_cell_is_reported() {
    let mut changed = reference();
    changed.labels("niches", &[0, 1, 1, 1]);
    assert_eq!(changed.differences(&reference(), 1e-12).len(), 1);
}

/// Floats get a tolerance, because a parallel sum does not add in the same
/// order twice and the last bits are not a result.
#[test]
fn a_float_within_tolerance_is_not_a_difference() {
    let mut drifted = reference();
    drifted.floats("assortativity", &[0.41 + 1e-13, -0.12, 0.0]);
    assert!(drifted.differences(&reference(), 1e-9).is_empty());
}

#[test]
fn a_float_beyond_tolerance_is_reported() {
    let mut drifted = reference();
    drifted.floats("assortativity", &[0.42, -0.12, 0.0]);

    let differences = drifted.differences(&reference(), 1e-9);
    assert_eq!(differences.len(), 1);
    assert!(differences[0].contains("assortativity"), "{differences:?}");
}

/// The tolerance is relative, so it means the same thing for a z-score of 180
/// as for a coefficient of 0.4.
#[test]
fn the_tolerance_is_relative_to_the_magnitude() {
    let mut small = Fingerprint::default();
    small.floats("z", &[180.0]);
    let mut drifted = Fingerprint::default();
    drifted.floats("z", &[180.0 * (1.0 + 1e-12)]);

    assert!(drifted.differences(&small, 1e-9).is_empty());
}

/// A grey cell of a mixing matrix is `NaN` in both runs; that is agreement,
/// not disagreement — and `NaN != NaN` would say otherwise.
#[test]
fn two_absent_values_agree() {
    let mut one = Fingerprint::default();
    one.floats("mixmat", &[f64::NAN, 0.5]);
    let mut two = Fingerprint::default();
    two.floats("mixmat", &[f64::NAN, 0.5]);

    assert!(one.differences(&two, 1e-12).is_empty());
}

/// A value that used to exist and no longer does is the most important
/// difference of all.
#[test]
fn a_value_that_became_absent_is_reported() {
    let mut one = Fingerprint::default();
    one.floats("mixmat", &[0.5, 0.5]);
    let mut two = Fingerprint::default();
    two.floats("mixmat", &[f64::NAN, 0.5]);

    assert_eq!(one.differences(&two, 1e-12).len(), 1);
}

#[test]
fn a_missing_stage_is_reported() {
    let mut partial = Fingerprint::default();
    partial.pairs("edges", &[(0, 1), (1, 2), (2, 3)]);

    let differences = partial.differences(&reference(), 1e-12);
    assert_eq!(differences.len(), 2, "{differences:?}");
}

#[test]
fn a_stage_of_a_different_length_is_reported() {
    let mut shorter = reference();
    shorter.floats("assortativity", &[0.41, -0.12]);

    let differences = shorter.differences(&reference(), 1e-12);
    assert_eq!(differences.len(), 1);
    assert!(differences[0].contains("length"), "{differences:?}");
}

// ---------------------------------------------------------------------------
// Fingerprints on disk
// ---------------------------------------------------------------------------

/// The reference lives in git, so it has to survive a round trip through a
/// file — including the `NaN` a mixing matrix is full of.
#[test]
fn a_fingerprint_survives_a_round_trip() {
    let mut original = reference();
    original.floats("with_absent", &[f64::NAN, 1.0]);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reference.json");
    original.save(&path).unwrap();

    let loaded = Fingerprint::load(&path).unwrap();
    assert!(
        loaded.differences(&original, 1e-12).is_empty(),
        "{:?}",
        loaded.differences(&original, 1e-12)
    );
}

#[test]
fn a_missing_reference_is_reported_rather_than_assumed_equal() {
    let dir = tempfile::tempdir().unwrap();
    let error = Fingerprint::load(dir.path().join("nothing.json")).unwrap_err();
    assert!(error.to_string().contains("nothing.json"), "{error}");
}
