//! Property tests for the Neighbors Aggregation Statistics.
//!
//! The NAS feature table is the input to every niche clustering, so a defect
//! here is invisible until the biology comes out wrong. These properties are
//! the ones a correct aggregation cannot violate.

use mosna_core::nas::adjacency::Adjacency;
use mosna_core::nas::bfs::BfsScratch;
use mosna_core::nas::{make_features_nas, one_hot};
use mosna_testkit::strategies::small_graph;
use proptest::prelude::*;

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

proptest! {
    /// The degrees of a CSR adjacency must sum to twice the number of edges:
    /// each undirected edge is stored in both directions, exactly once each.
    #[test]
    fn prop_adjacency_degrees_sum_to_twice_the_edges((n_nodes, edges) in small_graph(20)) {
        let adj = Adjacency::from_pairs(&edges, n_nodes);
        let total: usize = (0..n_nodes).map(|i| adj.degree(i)).sum();
        prop_assert_eq!(total, 2 * edges.len());
    }

    /// Adjacency is symmetric: `b` is a neighbour of `a` exactly when `a` is a
    /// neighbour of `b`.
    #[test]
    fn prop_adjacency_is_symmetric((n_nodes, edges) in small_graph(15)) {
        let adj = Adjacency::from_pairs(&edges, n_nodes);
        for a in 0..n_nodes {
            for &b in adj.neighbours(a) {
                prop_assert!(
                    adj.neighbours(b as usize).contains(&(a as u32)),
                    "{b} lists {a} but not the other way round"
                );
            }
        }
    }

    /// A neighbourhood always contains its own node, never repeats a node, and
    /// only ever contains valid indices.
    #[test]
    fn prop_neighbourhood_is_a_set_containing_self(
        (n_nodes, edges) in small_graph(15),
        order in 1usize..4,
    ) {
        let adj = Adjacency::from_pairs(&edges, n_nodes);
        let mut scratch = BfsScratch::new(n_nodes);

        for node in 0..n_nodes {
            let neighbourhood = scratch.neighbourhood(&adj, node, order).to_vec();
            prop_assert_eq!(neighbourhood[0], node as u32, "self must come first");

            let mut unique = neighbourhood.clone();
            unique.sort_unstable();
            unique.dedup();
            prop_assert_eq!(unique.len(), neighbourhood.len(), "a node was visited twice");
            prop_assert!(neighbourhood.iter().all(|&n| (n as usize) < n_nodes));
        }
    }

    /// Widening the order can only reach more nodes, never fewer.
    #[test]
    fn prop_neighbourhood_grows_with_order((n_nodes, edges) in small_graph(15)) {
        let adj = Adjacency::from_pairs(&edges, n_nodes);
        let mut scratch = BfsScratch::new(n_nodes);

        for node in 0..n_nodes {
            let narrow: std::collections::HashSet<u32> =
                scratch.neighbourhood(&adj, node, 1).iter().copied().collect();
            let wide: std::collections::HashSet<u32> =
                scratch.neighbourhood(&adj, node, 2).iter().copied().collect();
            prop_assert!(
                narrow.is_subset(&wide),
                "order 2 lost a node that order 1 reached, from {node}"
            );
        }
    }

    /// One-hot rows sum to one for every known label, and to zero otherwise.
    #[test]
    fn prop_one_hot_rows_sum_to_at_most_one(
        indices in proptest::collection::vec(0usize..5, 1..30)
    ) {
        let categories = names(&["a", "b", "c"]);
        let labels: Vec<Option<String>> = indices
            .iter()
            .map(|&i| Some(format!("{}", (b'a' + i as u8) as char)))
            .collect();

        let matrix = one_hot(&labels, &categories);
        for (row, chunk) in matrix.chunks(categories.len()).enumerate() {
            let sum: f64 = chunk.iter().sum();
            let known = indices[row] < categories.len();
            prop_assert_eq!(sum, if known { 1.0 } else { 0.0 });
            prop_assert!(chunk.iter().all(|&v| v == 0.0 || v == 1.0));
        }
    }

    /// Aggregating one-hot indicators yields a composition: the means across
    /// variables sum to one for every node, because each neighbour contributes
    /// exactly one unit spread over the categories.
    #[test]
    fn prop_aggregated_one_hot_means_form_a_composition(
        (n_nodes, edges) in small_graph(20),
        order in 1usize..3,
    ) {
        let categories = names(&["A", "B", "C"]);
        let labels: Vec<Option<String>> = (0..n_nodes)
            .map(|i| Some(categories[i % categories.len()].clone()))
            .collect();
        let x = one_hot(&labels, &categories);

        let features = make_features_nas(
            &x,
            n_nodes,
            &edges,
            order,
            &categories,
            &names(&["mean", "std"]),
            " ",
        );

        for row in 0..n_nodes {
            let means: f64 = features.row(row)[..categories.len()].iter().sum();
            prop_assert!(
                (means - 1.0).abs() < 1e-9,
                "row {row} means sum to {means}, not 1"
            );
        }
    }

    /// A standard deviation is never negative, and never `NaN` for finite input.
    #[test]
    fn prop_standard_deviations_are_non_negative(
        (n_nodes, edges) in small_graph(20),
        order in 1usize..3,
    ) {
        let categories = names(&["A", "B"]);
        let labels: Vec<Option<String>> = (0..n_nodes)
            .map(|i| Some(categories[i % 2].clone()))
            .collect();
        let x = one_hot(&labels, &categories);

        let features = make_features_nas(
            &x, n_nodes, &edges, order, &categories, &names(&["mean", "std"]), " ",
        );

        for value in &features.values {
            prop_assert!(value.is_finite(), "a feature is not finite");
        }
        for row in 0..n_nodes {
            for &std in &features.row(row)[categories.len()..] {
                prop_assert!(std >= 0.0, "a standard deviation is negative: {std}");
            }
        }
    }

    /// Every mean lies between the minimum and the maximum of the aggregated
    /// values — a mean outside that range means the wrong cells were summed.
    #[test]
    fn prop_means_stay_within_the_data_range(
        (n_nodes, edges) in small_graph(20),
        order in 1usize..3,
    ) {
        let x: Vec<f64> = (0..n_nodes).map(|i| (i as f64 * 0.7).sin()).collect();
        let features = make_features_nas(
            &x, n_nodes, &edges, order, &names(&["v"]), &names(&["mean"]), " ",
        );

        let lo = x.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for row in 0..n_nodes {
            let mean = features.row(row)[0];
            prop_assert!(
                mean >= lo - 1e-9 && mean <= hi + 1e-9,
                "mean {mean} is outside [{lo}, {hi}]"
            );
        }
    }

    /// The column layout is fixed by the variable and statistic names, whatever
    /// the network looks like.
    #[test]
    fn prop_column_names_are_the_cross_product(
        (n_nodes, edges) in small_graph(10),
        n_vars in 1usize..5,
    ) {
        let vars: Vec<String> = (0..n_vars).map(|i| format!("v{i}")).collect();
        let x = vec![1.0; n_nodes * n_vars];

        let features = make_features_nas(
            &x, n_nodes, &edges, 1, &vars, &names(&["mean", "std"]), " ",
        );

        prop_assert_eq!(features.column_names.len(), n_vars * 2);
        for (i, var) in vars.iter().enumerate() {
            prop_assert_eq!(&features.column_names[i], &format!("{var} mean"));
            prop_assert_eq!(&features.column_names[n_vars + i], &format!("{var} std"));
        }
    }
}
