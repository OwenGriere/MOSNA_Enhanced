//! Proptest strategies for the shapes MOSNA's algorithms accept.

use proptest::prelude::*;

/// A finite `f64` in a range wide enough to exercise the arithmetic but narrow
/// enough that a sum of a few hundred of them cannot overflow.
///
/// Unbounded `f64` generation is not useful here: every input to these
/// algorithms is a cell coordinate or a normalised count, and letting proptest
/// hand them `1e308` only tests overflow behaviour that the pipeline can never
/// reach.
pub fn finite_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        // Ordinary values.
        90 => -1.0e4f64..1.0e4f64,
        // Values near zero, where cancellation bites.
        10 => -1.0e-8f64..1.0e-8f64,
    ]
}

/// A cloud of `min_points..=max_points` distinct-ish 2-D points.
///
/// Coordinates are bounded so the Delaunay triangulation stays numerically
/// well conditioned, which is the regime the real cell coordinates live in.
pub fn point_cloud(min_points: usize, max_points: usize) -> impl Strategy<Value = Vec<[f64; 2]>> {
    proptest::collection::vec(
        (-1000.0f64..1000.0f64, -1000.0f64..1000.0f64).prop_map(|(x, y)| [x, y]),
        min_points..=max_points,
    )
}

/// An undirected graph over `n_nodes` nodes, given as a deduplicated edge list.
///
/// Self-loops are excluded and each pair is normalised to `(min, max)`, which
/// is the shape every consumer in the port expects.
pub fn small_graph(max_nodes: usize) -> impl Strategy<Value = (usize, Vec<(u32, u32)>)> {
    (2usize..=max_nodes.max(2)).prop_flat_map(|n_nodes| {
        let max_edges = (n_nodes * 3).min(60);
        (
            Just(n_nodes),
            proptest::collection::vec((0u32..n_nodes as u32, 0u32..n_nodes as u32), 0..=max_edges)
                .prop_map(|raw| {
                    let mut edges: Vec<(u32, u32)> = raw
                        .into_iter()
                        .filter(|(a, b)| a != b)
                        .map(|(a, b)| if a < b { (a, b) } else { (b, a) })
                        .collect();
                    edges.sort_unstable();
                    edges.dedup();
                    edges
                }),
        )
    })
}

/// A vector of `n` cluster labels drawn from `0..k`.
pub fn labels(n: usize, k: u32) -> impl Strategy<Value = Vec<u32>> {
    proptest::collection::vec(0u32..k.max(1), n..=n)
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn generated_floats_are_finite(x in finite_f64()) {
            prop_assert!(x.is_finite());
        }

        #[test]
        fn point_clouds_respect_their_bounds(cloud in point_cloud(3, 20)) {
            prop_assert!(cloud.len() >= 3 && cloud.len() <= 20);
            for point in cloud {
                prop_assert!(point[0].is_finite() && point[1].is_finite());
            }
        }

        #[test]
        fn graphs_are_simple_and_normalised((n_nodes, edges) in small_graph(15)) {
            for &(a, b) in &edges {
                prop_assert!(a < b, "pairs must be ordered");
                prop_assert!((b as usize) < n_nodes, "endpoints must be in range");
            }
            let mut sorted = edges.clone();
            sorted.sort_unstable();
            sorted.dedup();
            prop_assert_eq!(sorted.len(), edges.len(), "edges must be unique");
        }

        #[test]
        fn labels_stay_in_range(l in labels(10, 4)) {
            prop_assert_eq!(l.len(), 10);
            prop_assert!(l.iter().all(|&x| x < 4));
        }
    }
}
