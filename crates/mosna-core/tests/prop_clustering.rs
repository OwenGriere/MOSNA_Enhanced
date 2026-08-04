//! Tests for the niche clustering algorithms.
//!
//! Written before the implementations. Like UMAP, none of these have a
//! reference output to match — the Python seeds `leiden` with 0 but runs
//! `GaussianMixture` and `SpectralClustering` through libraries whose exact
//! trajectories are not reproducible here. What is pinned instead is what a
//! correct clusterer must do: recover structure that is unambiguously present,
//! return a well-formed partition, and give the same answer twice.

use mosna_core::clustering::{
    gaussian_mixture, leiden, merge_clusters, merge_clusters_until, relabel_clusters,
    spectral_clustering, GmmParams, SpectralParams,
};
use mosna_testkit::assert_valid_partition;
use mosna_testkit::fixtures::blobs;
use proptest::prelude::*;

/// How many distinct labels a partition uses.
fn n_clusters(labels: &[u32]) -> usize {
    let mut distinct: Vec<u32> = labels.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    distinct.len()
}

/// `true` when two labellings group the points identically, whatever names the
/// groups were given. Cluster ids are arbitrary, so this is the only sound way
/// to compare a clustering against a known truth.
fn same_grouping(a: &[u32], b: &[u32]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut forward = std::collections::HashMap::new();
    let mut backward = std::collections::HashMap::new();
    for (x, y) in a.iter().zip(b) {
        if *forward.entry(x).or_insert(y) != y {
            return false;
        }
        if *backward.entry(y).or_insert(x) != x {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// relabel_clusters — port of clustering.py::relabel_clusters
// ---------------------------------------------------------------------------

#[test]
fn relabel_makes_the_ids_contiguous() {
    // Gaps and an offset: the niche labels index colour maps and composition
    // matrices, so a gap would mis-colour a figure or index out of bounds.
    let labels = vec![5, 9, 5, 20, 9];
    let relabelled = relabel_clusters(&labels);
    assert_valid_partition(&relabelled, labels.len(), "relabelled");
    assert!(same_grouping(&labels, &relabelled));
}

#[test]
fn relabel_leaves_an_already_contiguous_partition_alone() {
    let labels = vec![0, 1, 2, 1, 0];
    assert_eq!(relabel_clusters(&labels), labels);
}

#[test]
fn relabel_handles_degenerate_inputs() {
    assert!(relabel_clusters(&[]).is_empty());
    assert_eq!(relabel_clusters(&[7]), vec![0]);
}

// ---------------------------------------------------------------------------
// merge_clusters — port of clustering.py::merge_clusters
// ---------------------------------------------------------------------------

#[test]
fn merging_absorbs_the_smallest_cluster_into_its_neighbour() {
    // Two large clusters and one stray point, sitting next to cluster 1.
    let coords = vec![
        0.0, 0.0, 0.1, 0.0, 0.2, 0.0, // cluster 0
        10.0, 0.0, 10.1, 0.0, 10.2, 0.0, // cluster 1
        10.3, 0.0, // the stray
    ];
    let labels = vec![0, 0, 0, 1, 1, 1, 2];

    let (merged, did_merge) = merge_clusters(&labels, &coords, 2, None, 25.0, 0.1, 10, true);
    assert!(did_merge);
    // The stray must have joined cluster 1, the one it is closest to.
    assert_eq!(merged[6], merged[3], "the stray joined the wrong cluster");
    assert_eq!(n_clusters(&merged), 2);
}

#[test]
fn merging_is_declined_when_every_cluster_is_big_enough() {
    let coords = vec![0.0, 0.0, 0.1, 0.0, 10.0, 0.0, 10.1, 0.0];
    let labels = vec![0, 0, 1, 1];
    let (unchanged, did_merge) = merge_clusters(&labels, &coords, 2, None, 25.0, 0.1, 10, false);
    assert!(!did_merge);
    assert_eq!(unchanged, labels);
}

#[test]
fn merging_until_a_target_reduces_the_cluster_count() {
    let (coords, truth) = blobs(5, 8, 12.0);
    let labels: Vec<u32> = truth;

    let merged = merge_clusters_until(&labels, &coords, 2, Some(3), true, None, 25.0, 0.1, 10);
    assert!(
        n_clusters(&merged) <= 3,
        "expected at most 3 clusters, got {}",
        n_clusters(&merged)
    );
    assert_valid_partition(&merged, labels.len(), "merged");
}

#[test]
fn merging_a_single_cluster_is_a_no_op() {
    let coords = vec![0.0, 0.0, 1.0, 1.0];
    let labels = vec![0, 0];
    let (unchanged, did_merge) = merge_clusters(&labels, &coords, 2, None, 25.0, 0.1, 10, true);
    assert!(!did_merge);
    assert_eq!(unchanged, labels);
}

// ---------------------------------------------------------------------------
// gaussian_mixture — replaces torchgmm / sklearn GaussianMixture
// ---------------------------------------------------------------------------

fn gmm_params(n_clusters: usize) -> GmmParams {
    GmmParams {
        n_clusters,
        seed: 7,
        ..GmmParams::default()
    }
}

#[test]
fn gmm_recovers_well_separated_blobs() {
    let (data, truth) = blobs(3, 40, 25.0);
    let result = gaussian_mixture(&data, 120, 2, &gmm_params(3)).unwrap();

    assert_valid_partition(&result.labels, 120, "gmm labels");
    assert!(
        same_grouping(&result.labels, &truth),
        "the mixture did not recover the blobs: {:?}",
        &result.labels[..12]
    );
}

#[test]
fn gmm_returns_one_mean_per_component() {
    let (data, _) = blobs(3, 30, 20.0);
    let result = gaussian_mixture(&data, 90, 2, &gmm_params(3)).unwrap();
    assert_eq!(result.means.len(), 3 * 2);
    assert!(result.means.iter().all(|v| v.is_finite()));
    assert!(result.log_likelihood.is_finite());
}

/// Expectation-maximisation cannot decrease the likelihood; that is the whole
/// guarantee of the algorithm. A run whose likelihood drops has a defect in the
/// M step.
#[test]
fn gmm_log_likelihood_never_decreases() {
    let (data, _) = blobs(4, 25, 15.0);
    let mut params = gmm_params(4);
    params.n_init = 1;
    let result = gaussian_mixture(&data, 100, 2, &params).unwrap();

    assert!(
        result
            .log_likelihood_history
            .windows(2)
            .all(|w| w[1] >= w[0] - 1e-9),
        "likelihood decreased: {:?}",
        result.log_likelihood_history
    );
}

#[test]
fn gmm_is_reproducible() {
    let (data, _) = blobs(3, 30, 20.0);
    let first = gaussian_mixture(&data, 90, 2, &gmm_params(3)).unwrap();
    let second = gaussian_mixture(&data, 90, 2, &gmm_params(3)).unwrap();
    assert_eq!(first.labels, second.labels);
    assert_eq!(first.means, second.means);
}

/// A component with fewer points than dimensions makes its covariance singular.
/// The regularisation must absorb that instead of producing a `NaN` likelihood.
#[test]
fn gmm_survives_a_collapsing_component() {
    // Ten identical points in five dimensions, asked for four components.
    let data = vec![1.0; 50];
    let result = gaussian_mixture(&data, 10, 5, &gmm_params(4)).unwrap();
    assert_valid_partition(&result.labels, 10, "labels");
    assert!(result.log_likelihood.is_finite());
}

#[test]
fn gmm_clamps_the_component_count_to_the_sample_size() {
    let (data, _) = blobs(1, 3, 1.0);
    let result = gaussian_mixture(&data, 3, 2, &gmm_params(10)).unwrap();
    assert!(n_clusters(&result.labels) <= 3);
}

// ---------------------------------------------------------------------------
// leiden — replaces leidenalg.find_partition
// ---------------------------------------------------------------------------

/// Three cliques joined by single edges: the community structure is not in
/// doubt.
fn three_cliques() -> (usize, Vec<(usize, usize, f64)>, Vec<u32>) {
    let mut edges = Vec::new();
    for clique in 0..3usize {
        for i in 0..6 {
            for j in (i + 1)..6 {
                edges.push((clique * 6 + i, clique * 6 + j, 1.0));
            }
        }
    }
    edges.push((5, 6, 1.0));
    edges.push((11, 12, 1.0));

    let truth: Vec<u32> = (0..18).map(|i| (i / 6) as u32).collect();
    (18, edges, truth)
}

#[test]
fn leiden_recovers_obvious_communities() {
    let (n_nodes, edges, truth) = three_cliques();
    let labels = leiden(n_nodes, &edges, 1.0, 0);

    assert_valid_partition(&labels, n_nodes, "leiden labels");
    assert!(
        same_grouping(&labels, &truth),
        "expected the three cliques, got {labels:?}"
    );
}

#[test]
fn leiden_is_reproducible() {
    let (n_nodes, edges, _) = three_cliques();
    assert_eq!(
        leiden(n_nodes, &edges, 1.0, 0),
        leiden(n_nodes, &edges, 1.0, 0)
    );
}

/// Resolution controls granularity: a higher value must never yield fewer
/// communities. This is the knob `resolution` in the configuration, so it has
/// to behave predictably.
#[test]
fn leiden_resolution_controls_granularity() {
    let (data, _) = blobs(6, 12, 15.0);
    let n_nodes = 72;
    // A neighbour graph over the blobs.
    let mut edges = Vec::new();
    for i in 0..n_nodes {
        for j in (i + 1)..n_nodes {
            let dx = data[i * 2] - data[j * 2];
            let dy = data[i * 2 + 1] - data[j * 2 + 1];
            if (dx * dx + dy * dy).sqrt() < 2.0 {
                edges.push((i, j, 1.0));
            }
        }
    }

    let coarse = n_clusters(&leiden(n_nodes, &edges, 0.05, 0));
    let fine = n_clusters(&leiden(n_nodes, &edges, 5.0, 0));
    assert!(
        fine >= coarse,
        "raising the resolution reduced the community count: {coarse} -> {fine}"
    );
}

#[test]
fn leiden_handles_a_graph_with_no_edges() {
    let labels = leiden(5, &[], 1.0, 0);
    assert_valid_partition(&labels, 5, "isolated nodes");
    // Every node is its own community.
    assert_eq!(n_clusters(&labels), 5);
}

#[test]
fn leiden_handles_an_empty_graph() {
    assert!(leiden(0, &[], 1.0, 0).is_empty());
}

// ---------------------------------------------------------------------------
// spectral_clustering — replaces sklearn.cluster.SpectralClustering
// ---------------------------------------------------------------------------

#[test]
fn spectral_recovers_well_separated_blobs() {
    let (data, truth) = blobs(3, 25, 12.0);
    let params = SpectralParams {
        n_clusters: 3,
        seed: 0,
        ..SpectralParams::default()
    };
    let labels = spectral_clustering(&data, 75, 2, &params).unwrap();

    assert_valid_partition(&labels, 75, "spectral labels");
    assert!(
        same_grouping(&labels, &truth),
        "spectral did not recover the blobs: {:?}",
        &labels[..10]
    );
}

/// Spectral clustering separates groups a centroid method cannot: two nested
/// rings share a centre, so k-means splits them by angle while the graph
/// Laplacian sees two components.
#[test]
fn spectral_separates_concentric_rings() {
    let mut data = Vec::new();
    let mut truth = Vec::new();
    for (ring, radius) in [(0u32, 1.0f64), (1, 6.0)] {
        for i in 0..40 {
            let t = i as f64 * std::f64::consts::TAU / 40.0;
            data.push(radius * t.cos());
            data.push(radius * t.sin());
            truth.push(ring);
        }
    }

    let params = SpectralParams {
        n_clusters: 2,
        // A tight kernel keeps the two rings from bridging.
        gamma: 2.0,
        seed: 0,
        ..SpectralParams::default()
    };
    let labels = spectral_clustering(&data, 80, 2, &params).unwrap();
    assert!(
        same_grouping(&labels, &truth),
        "the rings were not separated: {labels:?}"
    );
}

#[test]
fn spectral_is_reproducible() {
    let (data, _) = blobs(3, 20, 12.0);
    let params = SpectralParams {
        n_clusters: 3,
        seed: 4,
        ..SpectralParams::default()
    };
    let first = spectral_clustering(&data, 60, 2, &params).unwrap();
    let second = spectral_clustering(&data, 60, 2, &params).unwrap();
    assert_eq!(first, second);
}

/// The dense affinity matrix is `O(n^2)` in memory and the eigendecomposition
/// `O(n^3)` in time, exactly as in scikit-learn. Rather than exhausting memory
/// on a cohort-sized input, the limit is stated and refused with a message that
/// names the alternatives.
#[test]
fn spectral_refuses_an_input_beyond_its_size_limit() {
    let params = SpectralParams {
        n_clusters: 2,
        max_points: 100,
        ..SpectralParams::default()
    };
    let data = vec![0.0; 2 * 200];
    let err = spectral_clustering(&data, 200, 2, &params).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("200"), "{message}");
    assert!(
        message.contains("leiden") || message.contains("gmm"),
        "the error must point at a usable alternative: {message}"
    );
}

#[test]
fn spectral_handles_degenerate_inputs() {
    let params = SpectralParams {
        n_clusters: 3,
        seed: 0,
        ..SpectralParams::default()
    };
    // Fewer points than requested clusters.
    let data = vec![0.0, 0.0, 1.0, 1.0];
    let labels = spectral_clustering(&data, 2, 2, &params).unwrap();
    assert_valid_partition(&labels, 2, "two points");

    // Every point identical: the affinity matrix is constant.
    let data = vec![5.0; 20];
    let labels = spectral_clustering(&data, 10, 2, &params).unwrap();
    assert_valid_partition(&labels, 10, "coincident points");
}

proptest! {
    /// Whatever the input, spectral clustering returns a valid partition.
    #[test]
    fn prop_spectral_returns_a_valid_partition(
        raw in proptest::collection::vec(-20.0f64..20.0, 20..=50),
        k in 1usize..4,
    ) {
        let n_features = 2;
        let n_rows = raw.len() / n_features;
        prop_assume!(n_rows >= 2);
        let data = &raw[..n_rows * n_features];

        let params = SpectralParams {
            n_clusters: k,
            seed: 0,
            ..SpectralParams::default()
        };
        let labels = spectral_clustering(data, n_rows, n_features, &params).unwrap();
        assert_valid_partition(&labels, n_rows, "spectral");
    }

    /// Whatever the graph, the result is a well-formed partition.
    #[test]
    fn prop_leiden_returns_a_valid_partition(
        (n_nodes, raw) in mosna_testkit::strategies::small_graph(20),
    ) {
        let edges: Vec<(usize, usize, f64)> =
            raw.into_iter().map(|(a, b)| (a as usize, b as usize, 1.0)).collect();
        let labels = leiden(n_nodes, &edges, 1.0, 0);
        assert_valid_partition(&labels, n_nodes, "leiden");
    }

    /// Relabelling preserves the grouping and always produces contiguous ids.
    #[test]
    fn prop_relabel_preserves_grouping(
        labels in proptest::collection::vec(0u32..50, 1..40),
    ) {
        let relabelled = relabel_clusters(&labels);
        assert_valid_partition(&relabelled, labels.len(), "relabelled");
        prop_assert!(same_grouping(&labels, &relabelled));
    }

    /// The mixture always returns a valid partition with a finite likelihood.
    #[test]
    fn prop_gmm_returns_a_valid_partition(
        raw in proptest::collection::vec(-50.0f64..50.0, 20..=60),
        k in 1usize..5,
    ) {
        let n_features = 2;
        let n_rows = raw.len() / n_features;
        prop_assume!(n_rows >= 2);
        let data = &raw[..n_rows * n_features];

        let mut params = gmm_params(k);
        params.n_init = 1;
        params.max_iter = 25;

        let result = gaussian_mixture(data, n_rows, n_features, &params).unwrap();
        assert_valid_partition(&result.labels, n_rows, "gmm");
        prop_assert!(result.log_likelihood.is_finite());
        prop_assert!(result.means.iter().all(|v| v.is_finite()));
    }

    /// Merging never invents a cluster and never leaves a gap in the ids.
    #[test]
    fn prop_merging_only_reduces_the_cluster_count(
        raw in proptest::collection::vec(-20.0f64..20.0, 20..=40),
        k in 2u32..5,
    ) {
        let n_features = 2;
        let n_rows = raw.len() / n_features;
        prop_assume!(n_rows >= 4);
        let coords = &raw[..n_rows * n_features];
        let labels: Vec<u32> = (0..n_rows).map(|i| (i as u32) % k).collect();

        let before = n_clusters(&labels);
        let merged = merge_clusters_until(
            &labels, coords, n_features, None, false, None, 25.0, 0.1, 10,
        );
        assert_valid_partition(&merged, n_rows, "merged");
        prop_assert!(n_clusters(&merged) <= before);
    }
}
