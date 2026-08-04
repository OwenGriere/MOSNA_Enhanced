//! Tests of the agreement metrics, written before the implementation.
//!
//! Level 3 compares partitions that cannot be compared label by label: two runs
//! of Leiden may call the same group `2` and `5`. These metrics are what makes
//! "the same niches" a statement with a number attached, so they are checked
//! against values worked out by hand rather than against themselves.

use mosna_bench::agreement::{adjusted_rand_index, knn_overlap, normalized_mutual_information};

// ---------------------------------------------------------------------------
// Adjusted Rand index
// ---------------------------------------------------------------------------

#[test]
fn a_partition_agrees_perfectly_with_itself() {
    let labels = [0u32, 0, 1, 1, 2, 2, 2];
    assert!((adjusted_rand_index(&labels, &labels) - 1.0).abs() < 1e-12);
}

/// The whole point: the metric must not care what the groups are called.
#[test]
fn renaming_the_groups_changes_nothing() {
    let a = [0u32, 0, 1, 1, 2, 2];
    let b = [7u32, 7, 3, 3, 9, 9];
    assert!((adjusted_rand_index(&a, &b) - 1.0).abs() < 1e-12);
}

/// Worked out by hand on the standard four-item example.
///
/// `a = [0,0,1,1]`, `b = [0,1,0,1]`: every pair that is together in one is
/// apart in the other. The index of two partitions this opposed is negative.
#[test]
fn opposed_partitions_score_below_zero() {
    let a = [0u32, 0, 1, 1];
    let b = [0u32, 1, 0, 1];
    let score = adjusted_rand_index(&a, &b);
    assert!(score < 0.0, "expected a negative score, got {score}");
    assert!(
        score >= -1.0,
        "the index is bounded below by -1, got {score}"
    );
}

/// A partition that splits every item into its own group agrees with nothing.
#[test]
fn a_partition_into_singletons_agrees_with_nothing() {
    let grouped = [0u32, 0, 0, 1, 1, 1];
    let singletons = [0u32, 1, 2, 3, 4, 5];
    assert!(adjusted_rand_index(&grouped, &singletons).abs() < 1e-9);
}

/// The correction for chance is the reason to prefer this over the raw Rand
/// index: two independent random partitions must score around zero, not around
/// the 0.5 the uncorrected index would give.
#[test]
fn independent_partitions_score_around_zero() {
    let n = 600;
    let a: Vec<u32> = (0..n).map(|i| (i % 3) as u32).collect();
    // A deliberately unrelated grouping, built from a different modulus so the
    // two share no structure.
    let b: Vec<u32> = (0..n).map(|i| ((i * 7 + 1) % 4) as u32).collect();

    let score = adjusted_rand_index(&a, &b);
    assert!(score.abs() < 0.05, "expected about zero, got {score}");
}

/// A partial agreement lands strictly between the two extremes.
#[test]
fn a_partial_agreement_lands_in_between() {
    let truth = [0u32, 0, 0, 0, 1, 1, 1, 1];
    let mostly = [0u32, 0, 0, 1, 1, 1, 1, 1];
    let score = adjusted_rand_index(&truth, &mostly);
    assert!(score > 0.2 && score < 1.0, "got {score}");
}

#[test]
fn an_empty_partition_is_not_a_panic() {
    assert!(adjusted_rand_index(&[], &[]).is_nan());
}

// ---------------------------------------------------------------------------
// Normalised mutual information
// ---------------------------------------------------------------------------

#[test]
fn mutual_information_is_one_for_identical_partitions() {
    let labels = [0u32, 0, 1, 1, 2, 2];
    let score = normalized_mutual_information(&labels, &labels);
    assert!((score - 1.0).abs() < 1e-12, "got {score}");
}

#[test]
fn mutual_information_ignores_the_labels_themselves() {
    let a = [0u32, 0, 1, 1];
    let b = [5u32, 5, 0, 0];
    assert!((normalized_mutual_information(&a, &b) - 1.0).abs() < 1e-12);
}

/// A partition carrying no information about the other scores zero.
#[test]
fn an_uninformative_partition_scores_zero() {
    let a = [0u32, 0, 1, 1];
    let everything_together = [0u32, 0, 0, 0];
    assert!(normalized_mutual_information(&a, &everything_together).abs() < 1e-12);
}

#[test]
fn mutual_information_stays_within_its_bounds() {
    let a = [0u32, 0, 1, 1, 2, 0, 1, 2];
    let b = [1u32, 0, 1, 1, 2, 2, 1, 0];
    let score = normalized_mutual_information(&a, &b);
    assert!((0.0..=1.0).contains(&score), "got {score}");
}

// ---------------------------------------------------------------------------
// Neighbourhood preservation
// ---------------------------------------------------------------------------

/// The question level 3 asks of UMAP: are the neighbours in the projection the
/// neighbours of the original space? Labels cannot answer that; this can.
#[test]
fn an_identical_embedding_preserves_every_neighbour() {
    let data: Vec<f64> = (0..40).flat_map(|i| [i as f64, (i * i) as f64]).collect();
    let overlap = knn_overlap(&data, &data, 40, 2, 2, 5);
    assert!((overlap - 1.0).abs() < 1e-12, "got {overlap}");
}

/// A rigid transformation of the space keeps every neighbourhood.
#[test]
fn a_shifted_and_scaled_embedding_preserves_every_neighbour() {
    let data: Vec<f64> = (0..40).flat_map(|i| [i as f64, (i % 7) as f64]).collect();
    let moved: Vec<f64> = data.iter().map(|v| v * 3.0 + 100.0).collect();
    let overlap = knn_overlap(&data, &moved, 40, 2, 2, 5);
    assert!((overlap - 1.0).abs() < 1e-12, "got {overlap}");
}

/// An embedding that destroys the structure scores low.
#[test]
fn a_scrambled_embedding_preserves_almost_nothing() {
    let n = 120;
    let data: Vec<f64> = (0..n).flat_map(|i| [i as f64, 0.0]).collect();
    // Interleave the two halves: neighbours in the line end up far apart.
    let scrambled: Vec<f64> = (0..n)
        .flat_map(|i| {
            let moved = if i % 2 == 0 { i / 2 } else { n - 1 - i / 2 };
            [moved as f64, 0.0]
        })
        .collect();

    let overlap = knn_overlap(&data, &scrambled, n, 2, 2, 5);
    assert!(overlap < 0.5, "expected a poor score, got {overlap}");
}

#[test]
fn the_overlap_is_a_proportion() {
    let data: Vec<f64> = (0..30)
        .flat_map(|i| [i as f64, (i * 3 % 11) as f64])
        .collect();
    let other: Vec<f64> = (0..30)
        .flat_map(|i| [(i * 5 % 13) as f64, i as f64])
        .collect();
    let overlap = knn_overlap(&data, &other, 30, 2, 2, 4);
    assert!((0.0..=1.0).contains(&overlap), "got {overlap}");
}

/// Asking for more neighbours than there are points must not panic.
#[test]
fn asking_for_too_many_neighbours_is_survived() {
    let data: Vec<f64> = vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0];
    let overlap = knn_overlap(&data, &data, 3, 2, 2, 50);
    assert!((overlap - 1.0).abs() < 1e-12, "got {overlap}");
}
