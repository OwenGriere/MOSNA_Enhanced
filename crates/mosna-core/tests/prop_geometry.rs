//! Property tests for the spatial network reconstruction.
//!
//! Example-based tests pin known answers; these pin the invariants that must
//! hold for *every* input, which is what protects the edge list — written to
//! disk and consumed by every later step — from a malformed triangulation.

use mosna_core::geometry::{
    build_delaunay, build_knn, distance_neighbors, link_solitaries, remove_duplicate_pairs,
    LinkMethod, TrimDist,
};
use mosna_testkit::strategies::point_cloud;
use proptest::prelude::*;

/// An edge list is well formed when every pair is ordered, in range, unique,
/// and free of self-loops. Everything downstream assumes this.
fn assert_well_formed(pairs: &[(u32, u32)], n_nodes: usize, what: &str) {
    for &(a, b) in pairs {
        assert!(a < b, "{what}: pair ({a}, {b}) is not ordered");
        assert!(
            (b as usize) < n_nodes,
            "{what}: pair ({a}, {b}) is out of range for {n_nodes} nodes"
        );
    }
    let mut sorted = pairs.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), pairs.len(), "{what}: contains duplicates");
    assert_eq!(sorted, pairs, "{what}: is not sorted");
}

proptest! {
    #[test]
    fn prop_delaunay_edges_are_well_formed(coords in point_cloud(2, 40)) {
        let pairs = build_delaunay(&coords, TrimDist::None).unwrap();
        assert_well_formed(&pairs, coords.len(), "delaunay");
    }

    /// A planar graph on `n >= 3` vertices has at most `3n - 6` edges. The
    /// Delaunay triangulation is planar, so exceeding this means edges were
    /// emitted twice or a half-edge was mis-paired.
    #[test]
    fn prop_delaunay_is_planar(coords in point_cloud(3, 40)) {
        let pairs = build_delaunay(&coords, TrimDist::None).unwrap();
        let n = coords.len();
        prop_assert!(
            pairs.len() <= 3 * n - 6,
            "{} edges for {n} points exceeds the planar bound",
            pairs.len()
        );
    }

    /// Trimming can only remove edges, never invent them.
    #[test]
    fn prop_trimming_is_a_subset(coords in point_cloud(4, 40)) {
        let untrimmed = build_delaunay(&coords, TrimDist::None).unwrap();
        let trimmed = build_delaunay(&coords, TrimDist::default()).unwrap();

        prop_assert!(trimmed.len() <= untrimmed.len());
        let all: std::collections::HashSet<_> = untrimmed.iter().copied().collect();
        for pair in &trimmed {
            prop_assert!(all.contains(pair), "trimming produced a new edge {pair:?}");
        }
    }

    /// A stricter distance threshold can only keep fewer edges.
    #[test]
    fn prop_trimming_is_monotone_in_the_threshold(coords in point_cloud(4, 30)) {
        let loose = build_delaunay(&coords, TrimDist::Fixed(1e9)).unwrap();
        let tight = build_delaunay(&coords, TrimDist::Fixed(10.0)).unwrap();
        prop_assert!(tight.len() <= loose.len());
    }

    #[test]
    fn prop_knn_edges_are_well_formed(coords in point_cloud(2, 40), k in 1usize..6) {
        let pairs = build_knn(&coords, k);
        assert_well_formed(&pairs, coords.len(), "knn");
    }

    /// With `k >= 1` and at least two points, no node can be left isolated:
    /// every point has a nearest neighbour, and the edge is kept.
    #[test]
    fn prop_knn_leaves_no_isolated_node(coords in point_cloud(2, 30), k in 1usize..4) {
        let pairs = build_knn(&coords, k);
        let mut seen = vec![false; coords.len()];
        for &(a, b) in &pairs {
            seen[a as usize] = true;
            seen[b as usize] = true;
        }
        prop_assert!(seen.iter().all(|&s| s), "a node has no knn edge");
    }

    /// Relinking is purely additive: every edge that was there is still there.
    #[test]
    fn prop_link_solitaries_only_adds(
        coords in point_cloud(4, 30),
        min_neighbors in 1usize..4,
    ) {
        let initial = build_delaunay(&coords, TrimDist::default()).unwrap();
        for method in [LinkMethod::Delaunay, LinkMethod::Knn] {
            let relinked = link_solitaries(&coords, &initial, method, min_neighbors).unwrap();
            assert_well_formed(&relinked, coords.len(), "relinked");

            let after: std::collections::HashSet<_> = relinked.iter().copied().collect();
            for pair in &initial {
                prop_assert!(after.contains(pair), "{method:?} dropped edge {pair:?}");
            }
        }
    }

    /// Relinking is idempotent once every node it can serve has been served:
    /// running it twice changes nothing the second time.
    #[test]
    fn prop_link_solitaries_is_idempotent(
        coords in point_cloud(4, 25),
        min_neighbors in 1usize..3,
    ) {
        let initial = build_delaunay(&coords, TrimDist::default()).unwrap();
        let once = link_solitaries(&coords, &initial, LinkMethod::Delaunay, min_neighbors).unwrap();
        let twice = link_solitaries(&coords, &once, LinkMethod::Delaunay, min_neighbors).unwrap();
        prop_assert_eq!(once, twice);
    }

    /// Deduplication is idempotent and order-insensitive.
    #[test]
    fn prop_remove_duplicate_pairs_is_canonical(
        raw in proptest::collection::vec((0u32..20, 0u32..20), 0..40)
    ) {
        let once = remove_duplicate_pairs(raw.iter().copied());
        let twice = remove_duplicate_pairs(once.iter().copied());
        prop_assert_eq!(&once, &twice);

        // Feeding the input in reverse yields the same canonical form.
        let reversed = remove_duplicate_pairs(raw.iter().rev().copied());
        prop_assert_eq!(&once, &reversed);
    }

    /// Edge lengths are non-negative and independent of the endpoint order.
    #[test]
    fn prop_distances_are_symmetric_and_non_negative(coords in point_cloud(2, 20)) {
        let n = coords.len() as u32;
        let forward: Vec<(u32, u32)> = (0..n - 1).map(|i| (i, i + 1)).collect();
        let backward: Vec<(u32, u32)> = forward.iter().map(|&(a, b)| (b, a)).collect();

        let d_forward = distance_neighbors(&coords, &forward);
        let d_backward = distance_neighbors(&coords, &backward);

        for (a, b) in d_forward.iter().zip(&d_backward) {
            prop_assert!(*a >= 0.0, "a distance cannot be negative");
            prop_assert!((a - b).abs() < 1e-9, "distance depends on endpoint order");
        }
    }
}
