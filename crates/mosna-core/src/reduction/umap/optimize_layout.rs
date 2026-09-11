//! Stochastic gradient descent on the embedding coordinates.

use crate::reduction::umap::Scalar;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Gradients are clipped to this magnitude before being applied.
///
/// Same value and same reason as umap-learn: two points that land almost on top
/// of each other produce an enormous repulsive gradient, and without a clip a
/// single such pair throws the whole layout apart.
const GRADIENT_CLIP: Scalar = 4.0;

/// Optimise the layout by attracting connected points and repelling sampled
/// non-neighbours.
///
/// `edges` is the fuzzy simplicial set: `(a, b, weight)` with `weight` in
/// `(0, 1]`. A heavier edge is visited more often, which is how the weights
/// enter the objective — there is no per-edge weight in the gradient itself.
///
/// # Determinism
///
/// The updates are applied sequentially from a seeded generator, so a run is
/// reproducible. umap-learn parallelises this loop and accepts the resulting
/// race on the coordinates, which is one of the reasons its output is not
/// reproducible even with a fixed `random_state`. Sequential updates cost
/// throughput on very large inputs but make the embedding — and therefore every
/// niche label derived from it — repeatable.
#[allow(clippy::too_many_arguments)]
pub fn optimize_layout(
    embedding: &mut [Scalar],
    n_rows: usize,
    n_components: usize,
    edges: &[(usize, usize, Scalar)],
    n_epochs: usize,
    a: Scalar,
    b: Scalar,
    learning_rate: Scalar,
    negative_sample_rate: usize,
    repulsion_strength: Scalar,
    seed: u64,
) {
    if edges.is_empty() || n_rows < 2 || n_epochs == 0 {
        return;
    }

    let epochs_per_sample = make_epochs_per_sample(edges, n_epochs);
    let mut next_sample = epochs_per_sample.clone();
    let rate = negative_sample_rate.max(1) as Scalar;
    // The negative-sampling period is one division away from the positive one,
    // and division is deterministic — so it is recomputed where it is needed
    // rather than kept in a fourth array as long as the edge list. On a cohort
    // with a hundred and seventy million edges that array is 1.4 GB, and this
    // routine already holds three of them.
    let mut next_negative = epochs_per_sample
        .iter()
        .map(|e| e / rate)
        .collect::<Vec<Scalar>>();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    for epoch in 0..n_epochs {
        // The step size anneals to zero, which is what lets the layout settle.
        let alpha = learning_rate * (1.0 - epoch as Scalar / n_epochs as Scalar);

        for (index, &(head, tail, _)) in edges.iter().enumerate() {
            if next_sample[index] > epoch as Scalar {
                continue;
            }

            apply_attraction(embedding, n_components, head, tail, a, b, alpha);
            next_sample[index] += epochs_per_sample[index];

            // Repulsion against uniformly sampled points, as many as the
            // schedule has accumulated since the last visit.
            let epochs_per_negative = epochs_per_sample[index] / rate;
            let n_negative =
                ((epoch as Scalar - next_negative[index]) / epochs_per_negative) as usize;

            for _ in 0..n_negative {
                let other = rng.gen_range(0..n_rows);
                if other == head {
                    continue;
                }
                apply_repulsion(
                    embedding,
                    n_components,
                    head,
                    other,
                    a,
                    b,
                    alpha,
                    repulsion_strength,
                );
            }
            next_negative[index] += n_negative as Scalar * epochs_per_negative;
        }
    }
}

/// Pull two connected points together.
fn apply_attraction(
    embedding: &mut [Scalar],
    n_components: usize,
    head: usize,
    tail: usize,
    a: Scalar,
    b: Scalar,
    alpha: Scalar,
) {
    let distance = squared_distance(embedding, n_components, head, tail);
    let coefficient = if distance > 0.0 {
        -2.0 * a * b * distance.powf(b - 1.0) / (a * distance.powf(b) + 1.0)
    } else {
        0.0
    };

    for d in 0..n_components {
        let delta = embedding[head * n_components + d] - embedding[tail * n_components + d];
        let step = clip(coefficient * delta) * alpha;
        embedding[head * n_components + d] += step;
        // Both endpoints move: the edge is undirected.
        embedding[tail * n_components + d] -= step;
    }
}

/// Push a point away from a sampled non-neighbour.
#[allow(clippy::too_many_arguments)]
fn apply_repulsion(
    embedding: &mut [Scalar],
    n_components: usize,
    head: usize,
    other: usize,
    a: Scalar,
    b: Scalar,
    alpha: Scalar,
    repulsion_strength: Scalar,
) {
    let distance = squared_distance(embedding, n_components, head, other);
    let coefficient = if distance > 0.0 {
        // The `0.001 +` keeps the force finite for coincident points.
        2.0 * repulsion_strength * b / ((0.001 + distance) * (a * distance.powf(b) + 1.0))
    } else {
        0.0
    };

    for d in 0..n_components {
        let step = if coefficient > 0.0 {
            let delta = embedding[head * n_components + d] - embedding[other * n_components + d];
            clip(coefficient * delta)
        } else {
            // Exactly coincident: nudge in a fixed direction so the pair can
            // separate at all.
            GRADIENT_CLIP
        };
        embedding[head * n_components + d] += step * alpha;
    }
}

/// How many epochs pass between two visits of each edge.
///
/// The heaviest edge is visited every epoch; an edge of weight `w` is visited
/// every `n_epochs / (n_epochs * w / w_max)` epochs. An edge too weak to be
/// sampled even once gets a schedule beyond the run, so it is never visited.
fn make_epochs_per_sample(edges: &[(usize, usize, Scalar)], n_epochs: usize) -> Vec<Scalar> {
    let max_weight = edges
        .iter()
        .map(|&(_, _, w)| w)
        .fold(0.0, Scalar::max)
        .max(Scalar::MIN_POSITIVE);

    edges
        .iter()
        .map(|&(_, _, w)| {
            let visits = n_epochs as Scalar * w / max_weight;
            if visits > 0.0 {
                n_epochs as Scalar / visits
            } else {
                Scalar::INFINITY
            }
        })
        .collect()
}

#[inline]
fn squared_distance(embedding: &[Scalar], n_components: usize, i: usize, j: usize) -> Scalar {
    (0..n_components)
        .map(|d| {
            let delta = embedding[i * n_components + d] - embedding[j * n_components + d];
            delta * delta
        })
        .sum()
}

#[inline]
fn clip(value: Scalar) -> Scalar {
    value.clamp(-GRADIENT_CLIP, GRADIENT_CLIP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance(embedding: &[Scalar], n_components: usize, i: usize, j: usize) -> Scalar {
        squared_distance(embedding, n_components, i, j).sqrt()
    }

    #[test]
    fn attraction_pulls_connected_points_together() {
        let mut embedding = vec![-5.0, 0.0, 5.0, 0.0];
        let before = distance(&embedding, 2, 0, 1);

        // One heavy edge, no repulsion.
        optimize_layout(
            &mut embedding,
            2,
            2,
            &[(0, 1, 1.0)],
            200,
            1.577,
            0.895,
            1.0,
            0,
            1.0,
            0,
        );

        let after = distance(&embedding, 2, 0, 1);
        assert!(after < before, "distance went from {before} to {after}");
    }

    #[test]
    fn the_layout_stays_finite() {
        let mut embedding: Vec<Scalar> =
            (0..40).map(|i| (i as Scalar * 0.7).sin() * 10.0).collect();
        let edges: Vec<(usize, usize, Scalar)> = (0..19)
            .map(|i| (i, i + 1, 0.5 + 0.5 / (i + 1) as Scalar))
            .collect();

        optimize_layout(
            &mut embedding,
            20,
            2,
            &edges,
            100,
            1.577,
            0.895,
            1.0,
            5,
            1.0,
            0,
        );
        assert!(embedding.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn two_connected_groups_stay_apart() {
        // Two chains, no edge between them; repulsion must keep them separated.
        let mut embedding: Vec<Scalar> = (0..20)
            .flat_map(|i| {
                let group = if i < 10 { 0.0 } else { 6.0 };
                [group + (i % 10) as Scalar * 0.1, (i % 3) as Scalar * 0.1]
            })
            .collect();

        let mut edges: Vec<(usize, usize, Scalar)> = (0..9).map(|i| (i, i + 1, 1.0)).collect();
        edges.extend((10..19).map(|i| (i, i + 1, 1.0)));

        optimize_layout(
            &mut embedding,
            20,
            2,
            &edges,
            200,
            1.577,
            0.895,
            1.0,
            5,
            1.0,
            11,
        );

        let within = distance(&embedding, 2, 0, 1);
        let across = distance(&embedding, 2, 0, 15);
        assert!(
            across > within,
            "groups collapsed: within {within}, across {across}"
        );
    }

    #[test]
    fn the_optimisation_is_reproducible() {
        let start: Vec<Scalar> = (0..40).map(|i| (i as Scalar * 0.3).cos() * 10.0).collect();
        let edges: Vec<(usize, usize, Scalar)> = (0..19).map(|i| (i, i + 1, 1.0)).collect();

        let mut first = start.clone();
        let mut second = start;
        for embedding in [&mut first, &mut second] {
            optimize_layout(embedding, 20, 2, &edges, 50, 1.577, 0.895, 1.0, 5, 1.0, 9);
        }
        assert_eq!(first, second);
    }

    #[test]
    fn an_empty_problem_leaves_the_layout_alone() {
        let mut embedding = vec![1.0, 2.0, 3.0, 4.0];
        let untouched = embedding.clone();
        optimize_layout(&mut embedding, 2, 2, &[], 100, 1.5, 0.9, 1.0, 5, 1.0, 0);
        assert_eq!(embedding, untouched);

        optimize_layout(
            &mut embedding,
            2,
            2,
            &[(0, 1, 1.0)],
            0,
            1.5,
            0.9,
            1.0,
            5,
            1.0,
            0,
        );
        assert_eq!(embedding, untouched);
    }

    #[test]
    fn heavier_edges_are_visited_more_often() {
        let edges = vec![(0, 1, 1.0), (1, 2, 0.5), (2, 3, 0.25)];
        let schedule = make_epochs_per_sample(&edges, 100);
        assert!(schedule[0] < schedule[1] && schedule[1] < schedule[2]);
        // The heaviest edge is visited every epoch.
        assert!((schedule[0] - 1.0).abs() < 1e-12);
    }
}
