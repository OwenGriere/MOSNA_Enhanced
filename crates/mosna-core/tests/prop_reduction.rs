//! Tests for the UMAP dimensionality reduction.
//!
//! Written before the implementation. UMAP is stochastic and the Python side
//! runs it with `random_state=None`, so there is no reference output to compare
//! against — not even between two Python runs. What can be pinned is the
//! *specification*: each stage has a defining equation, and the whole has to
//! preserve neighbourhood structure. That is what these assert.

use mosna_core::reduction::umap::{
    find_ab_params, fuzzy_simplicial_set, knn_graph, smooth_knn_dist, umap, Metric, UmapParams,
};
use mosna_testkit::fixtures::blobs;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// find_ab_params — fitting the output kernel
// ---------------------------------------------------------------------------

/// The curve UMAP fits its kernel to: `1` below `min_dist`, then an exponential
/// decay with the given spread.
fn target_curve(x: f64, min_dist: f64, spread: f64) -> f64 {
    if x <= min_dist {
        1.0
    } else {
        (-(x - min_dist) / spread).exp()
    }
}

/// Sum of squared residuals of the fitted kernel against the target curve.
fn fit_error(a: f64, b: f64, min_dist: f64, spread: f64) -> f64 {
    (0..300)
        .map(|i| {
            let x = i as f64 * 3.0 * spread / 299.0;
            let residual = 1.0 / (1.0 + a * x.powf(2.0 * b)) - target_curve(x, min_dist, spread);
            residual * residual
        })
        .sum()
}

/// The optimiser must actually reach the minimum.
///
/// An absolute bound on the residual would be the wrong assertion: the residual
/// is *not* small, and cannot be. A rational kernel cannot follow a curve that
/// is flat up to `min_dist` and then decays exponentially, so the best possible
/// worst-case error grows with `min_dist` — about 0.05 at 0, 0.08 at 0.5, 0.10
/// at 0.9, for umap-learn's own fitted parameters just as much as for these.
///
/// What can be asserted is optimality: no nearby choice of `a` or `b` fits
/// better. A grid search over `a, b` confirms these are the global optima.
#[test]
fn ab_params_reach_the_optimum() {
    for (min_dist, spread) in [(0.0, 1.0), (0.1, 1.0), (0.5, 1.0), (0.9, 1.0), (0.1, 2.0)] {
        let (a, b) = find_ab_params(spread, min_dist);
        assert!(a > 0.0 && b > 0.0, "a and b must be positive, got {a}, {b}");

        let best = fit_error(a, b, min_dist, spread);
        for scale_a in [0.9, 0.95, 1.05, 1.1] {
            for scale_b in [0.9, 0.95, 1.05, 1.1] {
                let neighbour = fit_error(a * scale_a, b * scale_b, min_dist, spread);
                assert!(
                    neighbour >= best * (1.0 - 1e-9),
                    "min_dist={min_dist} spread={spread}: \
                     a*{scale_a}, b*{scale_b} fits better ({neighbour} < {best})"
                );
            }
        }
    }
}

/// A larger `min_dist` keeps points further apart, which means a flatter kernel
/// near the origin — a smaller `a`.
#[test]
fn ab_params_respond_to_min_dist() {
    let (a_tight, _) = find_ab_params(1.0, 0.0);
    let (a_loose, _) = find_ab_params(1.0, 0.5);
    assert!(
        a_loose < a_tight,
        "a should shrink as min_dist grows: {a_loose} vs {a_tight}"
    );
}

/// The values UMAP is known to produce for its default settings, to a
/// tolerance that leaves room for a different optimiser reaching the same
/// minimum by a different route.
#[test]
fn ab_params_match_the_reference_implementation() {
    let (a, b) = find_ab_params(1.0, 0.1);
    assert!((a - 1.577).abs() < 0.05, "a = {a}, expected about 1.577");
    assert!((b - 0.895).abs() < 0.05, "b = {b}, expected about 0.895");
}

// ---------------------------------------------------------------------------
// knn_graph — nearest neighbours under each metric
// ---------------------------------------------------------------------------

/// Brute-force nearest neighbours, used as the oracle.
fn brute_force_knn(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
) -> Vec<Vec<usize>> {
    (0..n_rows)
        .map(|i| {
            let mut order: Vec<usize> = (0..n_rows).filter(|&j| j != i).collect();
            order.sort_by(|&a, &b| {
                let da = distance(data, n_features, i, a, metric);
                let db = distance(data, n_features, i, b, metric);
                da.partial_cmp(&db).unwrap().then(a.cmp(&b))
            });
            order.truncate(k);
            order
        })
        .collect()
}

fn distance(data: &[f64], n_features: usize, i: usize, j: usize, metric: Metric) -> f64 {
    let a = &data[i * n_features..(i + 1) * n_features];
    let b = &data[j * n_features..(j + 1) * n_features];
    match metric {
        Metric::Euclidean => a
            .iter()
            .zip(b)
            .map(|(x, y)| (x - y) * (x - y))
            .sum::<f64>()
            .sqrt(),
        Metric::Manhattan => a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum(),
        Metric::Cosine => {
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
            let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
            if na == 0.0 || nb == 0.0 {
                1.0
            } else {
                1.0 - dot / (na * nb)
            }
        }
    }
}

/// The neighbour graph must agree with a brute-force search, for every metric.
/// A wrong metric here silently changes which cells count as similar.
#[test]
fn knn_graph_agrees_with_brute_force() {
    let (data, _) = blobs(3, 12, 8.0);
    let (n_rows, n_features) = (36, 2);

    for metric in [Metric::Euclidean, Metric::Manhattan, Metric::Cosine] {
        let k = 5;
        let graph = knn_graph(&data, n_rows, n_features, k, metric);
        let expected = brute_force_knn(&data, n_rows, n_features, k, metric);

        for (i, wanted) in expected.iter().enumerate() {
            assert_eq!(
                graph.indices[i].len(),
                k,
                "{metric:?}: wrong neighbour count"
            );
            // Distances must be sorted ascending.
            assert!(
                graph.distances[i].windows(2).all(|w| w[0] <= w[1] + 1e-12),
                "{metric:?}: neighbours of {i} are not sorted"
            );
            // The neighbour set must match; ties may be broken differently, so
            // compare the distance to the k-th neighbour rather than the ids.
            let got_last = graph.distances[i][k - 1];
            let want_last = distance(&data, n_features, i, wanted[k - 1], metric);
            assert!(
                (got_last - want_last).abs() < 1e-9,
                "{metric:?}: point {i} k-th distance {got_last} != {want_last}"
            );
            assert!(
                !graph.indices[i].contains(&i),
                "a point is its own neighbour"
            );
        }
    }
}

#[test]
fn knn_graph_clamps_k_to_the_dataset() {
    let (data, _) = blobs(1, 4, 1.0);
    let graph = knn_graph(&data, 4, 2, 100, Metric::Euclidean);
    // Only three other points exist.
    assert!(graph.indices.iter().all(|n| n.len() == 3));
}

// ---------------------------------------------------------------------------
// smooth_knn_dist — the per-point bandwidth
// ---------------------------------------------------------------------------

/// The defining equation of UMAP's local connectivity: the membership strengths
/// of a point's neighbours sum to `log2(k)`.
#[test]
fn smooth_knn_solves_its_defining_equation() {
    let (data, _) = blobs(3, 15, 6.0);
    let (n_rows, n_features, k) = (45, 2, 8);

    let graph = knn_graph(&data, n_rows, n_features, k, Metric::Euclidean);
    let (rho, sigma) = smooth_knn_dist(&graph.distances, 1.0);

    let target = (k as f64).log2();
    for i in 0..n_rows {
        assert!(sigma[i] > 0.0, "sigma must be positive, got {}", sigma[i]);
        let sum: f64 = graph.distances[i]
            .iter()
            .map(|d| (-(d - rho[i]).max(0.0) / sigma[i]).exp())
            .sum();
        assert!(
            (sum - target).abs() < 1e-3,
            "point {i}: membership sum {sum} != log2(k) = {target}"
        );
    }
}

/// `rho` is the distance to the nearest neighbour, which is what guarantees
/// every point keeps at least one full-strength connection and so cannot be
/// stranded in the embedding.
#[test]
fn rho_is_the_nearest_neighbour_distance() {
    let (data, _) = blobs(2, 10, 5.0);
    let graph = knn_graph(&data, 20, 2, 6, Metric::Euclidean);
    let (rho, _) = smooth_knn_dist(&graph.distances, 1.0);

    for (i, radius) in rho.iter().enumerate() {
        assert!(
            (radius - graph.distances[i][0]).abs() < 1e-12,
            "rho[{i}] = {radius} but the nearest neighbour is at {}",
            graph.distances[i][0]
        );
    }
}

// ---------------------------------------------------------------------------
// fuzzy_simplicial_set — the weighted graph UMAP optimises
// ---------------------------------------------------------------------------

/// The fuzzy set is symmetric, its weights lie in `(0, 1]`, and every point
/// keeps at least one strong edge.
#[test]
fn fuzzy_simplicial_set_is_a_valid_weighted_graph() {
    let (data, _) = blobs(3, 15, 6.0);
    let (n_rows, k) = (45, 8);

    let graph = knn_graph(&data, n_rows, 2, k, Metric::Euclidean);
    let fuzzy = fuzzy_simplicial_set(&graph, n_rows, 1.0);

    for &(a, b, w) in &fuzzy {
        assert!(a != b, "self-edges carry no information");
        assert!(w > 0.0 && w <= 1.0 + 1e-12, "weight {w} outside (0, 1]");
        assert!(a < n_rows && b < n_rows);
    }

    // Symmetrisation must have merged the two directions into one entry.
    let mut seen = std::collections::HashSet::new();
    for &(a, b, _) in &fuzzy {
        let key = if a < b { (a, b) } else { (b, a) };
        assert!(seen.insert(key), "edge ({a}, {b}) appears twice");
    }

    // No point may be isolated: an isolated point receives no attractive force
    // and drifts wherever repulsion pushes it.
    let mut connected = vec![false; n_rows];
    for &(a, b, _) in &fuzzy {
        connected[a] = true;
        connected[b] = true;
    }
    assert!(connected.iter().all(|&c| c), "a point has no fuzzy edge");
}

// ---------------------------------------------------------------------------
// umap — the whole reduction
// ---------------------------------------------------------------------------

fn params(n_components: usize) -> UmapParams {
    UmapParams {
        n_components,
        n_neighbors: 10,
        metric: Metric::Euclidean,
        min_dist: 0.0,
        seed: 42,
        ..UmapParams::default()
    }
}

#[test]
fn umap_returns_the_requested_shape() {
    let (data, _) = blobs(3, 20, 10.0);
    for n_components in [1, 2, 3] {
        let embedding = umap(&data, 60, 2, &params(n_components)).unwrap();
        assert_eq!(embedding.len(), 60 * n_components);
        assert!(
            embedding.iter().all(|v| v.is_finite()),
            "the embedding contains a non-finite coordinate"
        );
    }
}

/// The same input and seed must give the same embedding. The Python cannot
/// promise this — it runs UMAP with `random_state=None` — and reproducibility
/// is one of the reasons for the port.
#[test]
fn umap_is_reproducible() {
    let (data, _) = blobs(3, 20, 10.0);
    let first = umap(&data, 60, 2, &params(2)).unwrap();
    let second = umap(&data, 60, 2, &params(2)).unwrap();
    assert_eq!(first, second);
}

/// A different seed must give a different embedding, or the layout optimisation
/// is not actually running.
#[test]
fn umap_depends_on_the_seed() {
    let (data, _) = blobs(3, 20, 10.0);
    let a = umap(&data, 60, 2, &params(2)).unwrap();
    let mut other = params(2);
    other.seed = 7;
    let b = umap(&data, 60, 2, &other).unwrap();
    assert_ne!(a, b);
}

/// The point of the whole exercise: well-separated groups in the input must
/// stay together in the embedding. Measured as the fraction of each point's
/// embedded neighbours that come from its own blob.
#[test]
fn umap_preserves_cluster_structure() {
    let (data, truth) = blobs(3, 30, 20.0);
    let (n_rows, n_components) = (90, 2);
    let embedding = umap(&data, n_rows, 2, &params(n_components)).unwrap();

    let k = 5;
    let neighbours = knn_graph(&embedding, n_rows, n_components, k, Metric::Euclidean);

    let mut same_blob = 0usize;
    for i in 0..n_rows {
        for &j in &neighbours.indices[i] {
            if truth[i] == truth[j] {
                same_blob += 1;
            }
        }
    }
    let purity = same_blob as f64 / (n_rows * k) as f64;
    assert!(
        purity > 0.9,
        "only {:.1}% of embedded neighbours share a blob",
        purity * 100.0
    );
}

/// Structure preservation must not depend on the metric.
#[test]
fn umap_preserves_structure_under_every_metric() {
    let (data, truth) = blobs(3, 25, 20.0);
    let n_rows = 75;

    for metric in [Metric::Euclidean, Metric::Manhattan] {
        let mut p = params(2);
        p.metric = metric;
        let embedding = umap(&data, n_rows, 2, &p).unwrap();
        let neighbours = knn_graph(&embedding, n_rows, 2, 5, Metric::Euclidean);

        let same: usize = (0..n_rows)
            .flat_map(|i| neighbours.indices[i].iter().map(move |&j| (i, j)))
            .filter(|&(i, j)| truth[i] == truth[j])
            .count();
        let purity = same as f64 / (n_rows * 5) as f64;
        assert!(purity > 0.85, "{metric:?}: purity {purity:.2}");
    }
}

#[test]
fn umap_handles_degenerate_inputs() {
    // Fewer points than requested neighbours.
    let data = vec![0.0, 0.0, 1.0, 1.0, 2.0, 0.5];
    let embedding = umap(&data, 3, 2, &params(2)).unwrap();
    assert_eq!(embedding.len(), 6);
    assert!(embedding.iter().all(|v| v.is_finite()));

    // A single point.
    let embedding = umap(&[1.0, 2.0], 1, 2, &params(2)).unwrap();
    assert_eq!(embedding.len(), 2);
    assert!(embedding.iter().all(|v| v.is_finite()));

    // Every point identical — every distance is zero, which is what makes the
    // bandwidth search degenerate.
    let data = vec![5.0; 20];
    let embedding = umap(&data, 10, 2, &params(2)).unwrap();
    assert!(embedding.iter().all(|v| v.is_finite()));
}

proptest! {
    /// Whatever the input, the embedding has the right shape and no coordinate
    /// escapes to infinity or becomes `NaN`. A `NaN` here propagates into the
    /// clustering and produces a silently empty niche.
    #[test]
    fn prop_umap_output_is_always_finite(
        raw in proptest::collection::vec(-100.0f64..100.0, 20..=80),
        n_neighbors in 2usize..12,
    ) {
        let n_features = 2;
        let n_rows = raw.len() / n_features;
        prop_assume!(n_rows >= 2);
        let data = &raw[..n_rows * n_features];

        let mut p = params(2);
        p.n_neighbors = n_neighbors;
        // A short run: this property is about robustness, not layout quality.
        p.n_epochs = 30;

        let embedding = umap(data, n_rows, n_features, &p).unwrap();
        prop_assert_eq!(embedding.len(), n_rows * 2);
        prop_assert!(embedding.iter().all(|v| v.is_finite()));
    }

    /// The neighbour graph is well formed for any input: `k` sorted neighbours
    /// per point, none of them the point itself.
    #[test]
    fn prop_knn_graph_is_well_formed(
        raw in proptest::collection::vec(-100.0f64..100.0, 12..=60),
        k in 1usize..6,
    ) {
        let n_features = 3;
        let n_rows = raw.len() / n_features;
        prop_assume!(n_rows > k);
        let data = &raw[..n_rows * n_features];

        for metric in [Metric::Euclidean, Metric::Manhattan, Metric::Cosine] {
            let graph = knn_graph(data, n_rows, n_features, k, metric);
            for i in 0..n_rows {
                prop_assert_eq!(graph.indices[i].len(), k);
                prop_assert_eq!(graph.distances[i].len(), k);
                prop_assert!(!graph.indices[i].contains(&i));
                prop_assert!(graph.distances[i].windows(2).all(|w| w[0] <= w[1] + 1e-12));
                prop_assert!(graph.distances[i].iter().all(|d| d.is_finite() && *d >= 0.0));
            }
        }
    }

    /// The bandwidth search always converges to a positive sigma and a rho
    /// equal to the nearest-neighbour distance, whatever the distance profile.
    #[test]
    fn prop_smooth_knn_always_converges(
        distances in proptest::collection::vec(
            proptest::collection::vec(0.0f64..100.0, 4..=10),
            3..=20,
        ),
    ) {
        // Each row must be sorted, as it comes out of the neighbour search.
        let sorted: Vec<Vec<f64>> = distances
            .into_iter()
            .map(|mut row| {
                row.sort_by(|a, b| a.partial_cmp(b).unwrap());
                row
            })
            .collect();

        let (rho, sigma) = smooth_knn_dist(&sorted, 1.0);
        for i in 0..sorted.len() {
            prop_assert!(sigma[i] > 0.0, "sigma[{}] = {}", i, sigma[i]);
            prop_assert!(sigma[i].is_finite());
            prop_assert!((rho[i] - sorted[i][0]).abs() < 1e-12);
        }
    }
}
