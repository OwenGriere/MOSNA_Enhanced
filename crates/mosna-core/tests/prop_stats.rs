//! Property tests for the statistical helpers.

use mosna_core::stats::clr::{closure, transform_clr};
use mosna_core::stats::{clr, dendrogram_leaf_order, percentile, ward_linkage};
use proptest::prelude::*;

fn values(min: usize, max: usize) -> impl Strategy<Value = Vec<f64>> {
    proptest::collection::vec(-1000.0f64..1000.0, min..=max)
}

fn points(n: usize, dim: usize) -> impl Strategy<Value = Vec<Vec<f64>>> {
    proptest::collection::vec(proptest::collection::vec(-50.0f64..50.0, dim..=dim), n..=n)
}

proptest! {
    /// A percentile always lies between the minimum and the maximum.
    #[test]
    fn prop_percentile_is_within_the_data_range(v in values(1, 60), q in 0.0f64..100.0) {
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let p = percentile(&v, q).unwrap();
        prop_assert!(p >= lo - 1e-9 && p <= hi + 1e-9, "{p} outside [{lo}, {hi}]");
    }

    /// Percentiles are non-decreasing in `q`. This is what makes the adaptive
    /// edge-trimming threshold behave predictably as the network grows.
    #[test]
    fn prop_percentile_is_monotone_in_q(v in values(2, 40), a in 0.0f64..100.0, b in 0.0f64..100.0) {
        let (lo_q, hi_q) = if a <= b { (a, b) } else { (b, a) };
        let lo = percentile(&v, lo_q).unwrap();
        let hi = percentile(&v, hi_q).unwrap();
        prop_assert!(lo <= hi + 1e-9, "p({lo_q}) = {lo} > p({hi_q}) = {hi}");
    }

    /// The endpoints are exactly the minimum and the maximum.
    #[test]
    fn prop_percentile_endpoints_are_extrema(v in values(1, 40)) {
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        prop_assert!((percentile(&v, 0.0).unwrap() - lo).abs() < 1e-12);
        prop_assert!((percentile(&v, 100.0).unwrap() - hi).abs() < 1e-12);
    }

    /// The percentile does not depend on the input order.
    #[test]
    fn prop_percentile_ignores_order(v in values(2, 30), q in 0.0f64..100.0) {
        let mut reversed = v.clone();
        reversed.reverse();
        let a = percentile(&v, q).unwrap();
        let b = percentile(&reversed, q).unwrap();
        prop_assert!((a - b).abs() < 1e-12);
    }

    /// Closure turns each row into a composition summing to one.
    #[test]
    fn prop_closure_normalises_rows(rows in points(6, 4)) {
        // Use magnitudes, since a composition is made of non-negative parts.
        let mut m: Vec<Vec<f64>> = rows.iter().map(|r| r.iter().map(|v| v.abs()).collect()).collect();
        closure(&mut m);

        for row in &m {
            let sum: f64 = row.iter().sum();
            prop_assert!(
                sum.abs() < 1e-12 || (sum - 1.0).abs() < 1e-9,
                "a closed row sums to {sum}"
            );
        }
    }

    /// The centred log-ratio transform centres each row on zero.
    #[test]
    fn prop_clr_rows_are_centred(rows in points(5, 4)) {
        let mut m: Vec<Vec<f64>> = rows
            .iter()
            .map(|r| r.iter().map(|v| v.abs() + 0.1).collect())
            .collect();
        closure(&mut m);
        clr(&mut m);

        for row in &m {
            let sum: f64 = row.iter().sum();
            prop_assert!(sum.abs() < 1e-8, "clr row sums to {sum}");
        }
    }

    /// The full transform never produces a non-finite value, even when the
    /// counts matrix is full of zeros — the case `ln(0)` would blow up on.
    #[test]
    fn prop_transform_clr_stays_finite(rows in points(5, 4)) {
        let mut m: Vec<Vec<f64>> = rows
            .iter()
            .map(|r| r.iter().map(|v| if *v > 0.0 { *v } else { 0.0 }).collect())
            .collect();
        transform_clr(&mut m);

        for row in &m {
            for value in row {
                prop_assert!(value.is_finite(), "clr produced {value}");
            }
        }
    }

    /// A dendrogram over `n` leaves has exactly `n - 1` merges, non-decreasing
    /// distances, and only ever refers to clusters that already exist.
    #[test]
    fn prop_linkage_is_a_well_formed_dendrogram(rows in points(9, 3)) {
        let linkage = ward_linkage(&rows);
        prop_assert_eq!(linkage.merges.len(), rows.len() - 1);
        prop_assert_eq!(linkage.n_leaves, rows.len());

        for (step, merge) in linkage.merges.iter().enumerate() {
            let created = linkage.n_leaves + step;
            prop_assert!(
                merge.left < created && merge.right < created,
                "step {step} refers to a cluster that does not exist yet"
            );
            prop_assert!(merge.left < merge.right, "merge is not ordered");
            prop_assert!(merge.distance >= 0.0 && merge.distance.is_finite());
        }

        prop_assert!(
            linkage.merges.windows(2).all(|w| w[0].distance <= w[1].distance + 1e-9),
            "Ward linkage must be monotone"
        );
    }

    /// The final merge gathers every leaf.
    #[test]
    fn prop_linkage_ends_with_every_leaf(rows in points(7, 2)) {
        let linkage = ward_linkage(&rows);
        prop_assert_eq!(linkage.merges.last().unwrap().count, rows.len());
    }

    /// The leaf order is a permutation of the leaves — no leaf duplicated, none
    /// lost. A heatmap indexed by a non-permutation would silently drop rows.
    #[test]
    fn prop_leaf_order_is_a_permutation(rows in points(8, 3)) {
        let order = dendrogram_leaf_order(&ward_linkage(&rows));
        prop_assert_eq!(order.len(), rows.len());

        let mut sorted = order.clone();
        sorted.sort_unstable();
        prop_assert_eq!(sorted, (0..rows.len()).collect::<Vec<_>>());
    }

    /// Clustering the same points twice gives the same dendrogram.
    #[test]
    fn prop_linkage_is_reproducible(rows in points(7, 3)) {
        prop_assert_eq!(ward_linkage(&rows), ward_linkage(&rows));
    }
}
