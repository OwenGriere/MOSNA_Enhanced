//! Property tests for the assortativity analysis.

use mosna_core::assortativity::{
    attribute_ac, attributes_pairs, mixing_matrix, mixmat_to_columns, sample_assort_mixmat,
    series_to_mixmat, zscore,
};
use mosna_testkit::assert_symmetric;
use mosna_testkit::strategies::small_graph;
use proptest::prelude::*;

/// Assign attributes cyclically, so abundances are known and reproducible.
fn assignments(n_nodes: usize, n_attributes: usize) -> Vec<Option<u32>> {
    (0..n_nodes)
        .map(|i| Some((i % n_attributes.max(1)) as u32))
        .collect()
}

fn attribute_names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("p{i}")).collect()
}

proptest! {
    #[test]
    fn prop_mixing_matrix_is_symmetric(
        (n_nodes, edges) in small_graph(20),
        n_attributes in 1usize..5,
    ) {
        let a = assignments(n_nodes, n_attributes);
        let m = mixing_matrix(&a, &edges, n_attributes, false, true);
        assert_symmetric(&m.values, n_attributes, 1e-12, "mixing matrix");
    }

    /// Unnormalised, the matrix accounts for every edge exactly twice — once in
    /// each direction. Losing or double-counting an edge shows up here.
    #[test]
    fn prop_mixing_matrix_conserves_edge_mass(
        (n_nodes, edges) in small_graph(20),
        n_attributes in 1usize..5,
    ) {
        let a = assignments(n_nodes, n_attributes);
        let m = mixing_matrix(&a, &edges, n_attributes, false, true);
        prop_assert_eq!(m.sum(), 2.0 * edges.len() as f64);
    }

    /// Normalising makes the matrix a probability distribution, unless there
    /// are no edges at all.
    #[test]
    fn prop_normalised_matrix_sums_to_one(
        (n_nodes, edges) in small_graph(20),
        n_attributes in 1usize..5,
    ) {
        let a = assignments(n_nodes, n_attributes);
        let m = mixing_matrix(&a, &edges, n_attributes, true, true);
        if edges.is_empty() {
            prop_assert_eq!(m.sum(), 0.0);
        } else {
            prop_assert!((m.sum() - 1.0).abs() < 1e-12);
            prop_assert!(m.values.iter().all(|&v| v >= 0.0));
        }
    }

    /// Newman's coefficient is bounded by 1 above. It can fall below -1 only in
    /// the degenerate direction, so the useful bound to pin is the upper one
    /// plus finiteness whenever the network has edges and more than one
    /// attribute.
    #[test]
    fn prop_assortativity_is_bounded_above_by_one(
        (n_nodes, edges) in small_graph(20),
        n_attributes in 2usize..5,
    ) {
        let a = assignments(n_nodes, n_attributes);
        let m = mixing_matrix(&a, &edges, n_attributes, true, true);
        let r = attribute_ac(&m);
        if r.is_finite() {
            prop_assert!(r <= 1.0 + 1e-9, "assortativity {r} exceeds 1");
        }
    }

    /// The coefficient does not depend on how the matrix is scaled: the Python
    /// normalises inside the function, so a raw count matrix and its normalised
    /// form must agree.
    #[test]
    fn prop_assortativity_is_scale_invariant(
        (n_nodes, edges) in small_graph(20),
        n_attributes in 2usize..5,
    ) {
        prop_assume!(!edges.is_empty());
        let a = assignments(n_nodes, n_attributes);
        let raw = mixing_matrix(&a, &edges, n_attributes, false, true);
        let normalised = mixing_matrix(&a, &edges, n_attributes, true, true);

        let (r_raw, r_norm) = (attribute_ac(&raw), attribute_ac(&normalised));
        if r_raw.is_finite() && r_norm.is_finite() {
            prop_assert!((r_raw - r_norm).abs() < 1e-9, "{r_raw} != {r_norm}");
        }
    }

    /// A network whose nodes all share one attribute is perfectly assortative
    /// by construction; the coefficient is undefined there (0/0), and the code
    /// must say so rather than return a misleading number.
    #[test]
    fn prop_single_attribute_is_undefined((n_nodes, edges) in small_graph(15)) {
        prop_assume!(!edges.is_empty());
        let a = vec![Some(0u32); n_nodes];
        let m = mixing_matrix(&a, &edges, 1, true, true);
        prop_assert!(attribute_ac(&m).is_nan());
    }

    /// The flattened lower triangle and the generated pair names always have
    /// the same length — this is what keeps `net_stat.csv` well formed.
    #[test]
    fn prop_flattening_matches_the_pair_names(n_attributes in 1usize..12) {
        let names = attribute_names(n_attributes);
        let m = mosna_core::assortativity::MixMat::zeros(n_attributes);
        prop_assert_eq!(
            mixmat_to_columns(&m).len(),
            attributes_pairs(&names, "", " - ", " Z").len()
        );
    }

    /// Rebuilding a matrix from named pairs yields a symmetric matrix carrying
    /// every value that was named.
    #[test]
    fn prop_series_to_mixmat_is_symmetric(n_attributes in 2usize..8) {
        let names = attributes_pairs(&attribute_names(n_attributes), "", " - ", " Z");
        let values: Vec<f64> = (0..names.len()).map(|i| i as f64 * 0.5 - 1.0).collect();

        let (labels, m) = series_to_mixmat(&names, &values, " - ", " Z");
        prop_assert_eq!(labels.len(), n_attributes);
        assert_symmetric(&m.values, labels.len(), 1e-12, "rebuilt matrix");

        // Every named value must appear somewhere in the matrix.
        for value in &values {
            prop_assert!(
                m.values.iter().any(|v| (v - value).abs() < 1e-12),
                "value {value} was lost"
            );
        }
    }

    /// The z-score of the null's own mean is zero, and scaling the whole
    /// problem leaves the z-score unchanged — it is a standardised quantity.
    #[test]
    fn prop_zscore_is_standardised(
        sample in proptest::collection::vec(-100.0f64..100.0, 2..40),
        scale in 0.1f64..10.0,
    ) {
        let mean = sample.iter().sum::<f64>() / sample.len() as f64;
        let (reported_mean, std, z_at_mean) = zscore(mean, &sample);
        prop_assert!((reported_mean - mean).abs() < 1e-9);

        if std > 1e-9 {
            prop_assert!(z_at_mean.abs() < 1e-6, "z at the mean is {z_at_mean}");

            // Scaling observation and null together must not move the z-score.
            let scaled: Vec<f64> = sample.iter().map(|v| v * scale).collect();
            let (_, _, z_scaled) = zscore(sample[0] * scale, &scaled);
            let (_, _, z_plain) = zscore(sample[0], &sample);
            prop_assert!((z_scaled - z_plain).abs() < 1e-6);
        }
    }

    /// A full sample row is internally consistent: as many values as names, a
    /// node count matching the input, and abundances that sum to one.
    #[test]
    fn prop_sample_stats_are_self_consistent(
        (n_nodes, edges) in small_graph(16),
        n_attributes in 2usize..4,
    ) {
        let a = assignments(n_nodes, n_attributes);
        let names = attribute_names(n_attributes);
        let stats = sample_assort_mixmat(&a, &edges, &names, "patient-1_sample-1", 8);

        prop_assert_eq!(stats.values.len(), stats.column_names.len());
        prop_assert_eq!(stats.values[0], n_nodes as f64);

        let abundance: f64 = stats.values[1..=n_attributes].iter().sum();
        prop_assert!(
            (abundance - 1.0).abs() < 1e-9,
            "abundances sum to {abundance}"
        );
    }

    /// Two runs on the same input agree exactly, including the permutation
    /// null: reproducibility is a promise the port makes and the Python does
    /// not.
    #[test]
    fn prop_sample_stats_are_reproducible(
        (n_nodes, edges) in small_graph(14),
        n_attributes in 2usize..4,
    ) {
        let a = assignments(n_nodes, n_attributes);
        let names = attribute_names(n_attributes);
        let first = sample_assort_mixmat(&a, &edges, &names, "s", 12);
        let second = sample_assort_mixmat(&a, &edges, &names, "s", 12);

        for (x, y) in first.values.iter().zip(&second.values) {
            prop_assert!(
                (x == y) || (x.is_nan() && y.is_nan()),
                "run-to-run difference: {x} vs {y}"
            );
        }
    }
}
