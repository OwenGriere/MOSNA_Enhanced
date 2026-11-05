//! Stochastic gradient descent on the embedding coordinates.

use crate::reduction::umap::Scalar;

use rayon::prelude::*;

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
/// # Parallel, and still deterministic
///
/// umap-learn parallelises this loop by letting threads write the coordinates
/// concurrently and accepting the race, which is one of the reasons its output
/// is not reproducible even with a fixed `random_state`. Here every epoch is
/// computed from the layout as it stood when the epoch began:
///
/// 1. each point gathers the steps of the edges it is an endpoint of — its own
///    attractions and repulsions as head, the opposite attraction as tail —
///    reading only that frozen layout, so points are independent and run in
///    parallel;
/// 2. the steps of one point are added in a fixed order, edges as head then
///    edges as tail, each in edge order — never in the order threads finish;
/// 3. the negative samples are drawn from a counter-based hash of
///    `(seed, epoch, edge, draw)` rather than from one shared generator, so a
///    draw does not depend on which edge another thread reached first.
///
/// Same input and seed, same embedding, whatever the core count. The layout is
/// not the one the former sequential loop produced — a step no longer sees the
/// steps taken before it in the same epoch — which is why [`LAYOUT_REVISION`]
/// exists.
///
/// # Memory
///
/// Beside the three schedule arrays already held per edge, this keeps the
/// incidence of each point as tail — one `u32` per edge — and a second copy of
/// the layout. The incidence as head is free: the edges come sorted by head
/// from [`fn@crate::reduction::umap::fuzzy_simplicial_set`], and are only
/// indexed when a caller passes them in another order.
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
    assert!(
        edges.len() <= u32::MAX as usize,
        "the layout indexes edges as u32; {} edges is beyond it",
        edges.len()
    );

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

    let as_head = Incidence::new(n_rows, edges, |&(head, _, _)| head);
    let as_tail = Incidence::new(n_rows, edges, |&(_, tail, _)| tail);
    let mut frozen = embedding.to_vec();

    for epoch in 0..n_epochs {
        // The step size anneals to zero, which is what lets the layout settle.
        let alpha = learning_rate * (1.0 - epoch as Scalar / n_epochs as Scalar);
        let now = epoch as Scalar;
        frozen.copy_from_slice(embedding);

        let schedule = Schedule {
            edges,
            next_sample: &next_sample,
            next_negative: &next_negative,
            epochs_per_sample: &epochs_per_sample,
            rate,
            now,
        };
        let force = Force {
            a,
            b,
            alpha,
            repulsion_strength,
        };

        embedding
            .par_chunks_mut(n_components)
            .enumerate()
            .for_each(|(point, out)| {
                for edge in as_head.of(point) {
                    if !schedule.active(edge) {
                        continue;
                    }
                    let (_, tail, _) = edges[edge];
                    force.attract(&frozen, n_components, point, tail, out, 1.0);

                    for draw in 0..schedule.n_negative(edge) {
                        let other = negative_sample(seed, epoch, edge, draw, n_rows);
                        if other == point {
                            continue;
                        }
                        force.repel(&frozen, n_components, point, other, out);
                    }
                }

                for edge in as_tail.of(point) {
                    if !schedule.active(edge) {
                        continue;
                    }
                    let (head, _, _) = edges[edge];
                    // The opposite of the step the head takes, as both
                    // endpoints of an undirected edge move.
                    force.attract(&frozen, n_components, head, point, out, -1.0);
                }
            });

        // The schedule moves on only once every point has read it.
        next_sample
            .par_iter_mut()
            .zip(next_negative.par_iter_mut())
            .zip(epochs_per_sample.par_iter())
            .for_each(|((next, negative), &period)| {
                if *next > now {
                    return;
                }
                *next += period;
                let per_negative = period / rate;
                let n_negative = ((now - *negative) / per_negative) as usize;
                *negative += n_negative as Scalar * per_negative;
            });
    }
}

/// Bumped whenever [`fn@optimize_layout`] changes what it returns for the same
/// input, so that a cached embedding from an earlier revision is recomputed
/// rather than believed.
///
/// - 1: sequential stochastic gradient descent from one generator.
/// - 2: each epoch from a frozen layout, in parallel over points, with negative
///   samples from a counter-based hash.
pub const LAYOUT_REVISION: u32 = 2;

/// Where each edge stands in its schedule at the current epoch.
struct Schedule<'a> {
    edges: &'a [(usize, usize, Scalar)],
    next_sample: &'a [Scalar],
    next_negative: &'a [Scalar],
    epochs_per_sample: &'a [Scalar],
    rate: Scalar,
    now: Scalar,
}

impl Schedule<'_> {
    fn active(&self, edge: usize) -> bool {
        debug_assert!(edge < self.edges.len());
        self.next_sample[edge] <= self.now
    }

    /// Repulsions owed to an active edge, as many as the schedule has
    /// accumulated since its last visit.
    fn n_negative(&self, edge: usize) -> usize {
        let per_negative = self.epochs_per_sample[edge] / self.rate;
        ((self.now - self.next_negative[edge]) / per_negative) as usize
    }
}

/// The constants of the two forces at the current epoch.
struct Force {
    a: Scalar,
    b: Scalar,
    alpha: Scalar,
    repulsion_strength: Scalar,
}

impl Force {
    /// Add to `out` the attraction `head` feels towards `tail`, times `sign`.
    fn attract(
        &self,
        frozen: &[Scalar],
        n_components: usize,
        head: usize,
        tail: usize,
        out: &mut [Scalar],
        sign: Scalar,
    ) {
        let distance = squared_distance(frozen, n_components, head, tail);
        let coefficient = if distance > 0.0 {
            -2.0 * self.a * self.b * distance.powf(self.b - 1.0)
                / (self.a * distance.powf(self.b) + 1.0)
        } else {
            0.0
        };
        for (d, value) in out.iter_mut().enumerate() {
            let delta = frozen[head * n_components + d] - frozen[tail * n_components + d];
            *value += sign * clip(coefficient * delta) * self.alpha;
        }
    }

    /// Add to `out` the repulsion `head` feels from `other`.
    fn repel(
        &self,
        frozen: &[Scalar],
        n_components: usize,
        head: usize,
        other: usize,
        out: &mut [Scalar],
    ) {
        let distance = squared_distance(frozen, n_components, head, other);
        let coefficient = if distance > 0.0 {
            // The `0.001 +` keeps the force finite for coincident points.
            2.0 * self.repulsion_strength * self.b
                / ((0.001 + distance) * (self.a * distance.powf(self.b) + 1.0))
        } else {
            0.0
        };
        for (d, value) in out.iter_mut().enumerate() {
            let step = if coefficient > 0.0 {
                let delta = frozen[head * n_components + d] - frozen[other * n_components + d];
                clip(coefficient * delta)
            } else {
                // Exactly coincident: nudge in a fixed direction so the pair
                // can separate at all.
                GRADIENT_CLIP
            };
            *value += step * self.alpha;
        }
    }
}

/// The edges each point is an endpoint of, in edge order.
enum Incidence {
    /// The edges are sorted by this endpoint: each point's edges are a
    /// contiguous run, and an index would only cost memory.
    Sorted(Vec<usize>),
    /// A counting sort of the edge ids by this endpoint — stable, so each
    /// point's edges stay in edge order.
    Indexed {
        offsets: Vec<usize>,
        edges: Vec<u32>,
    },
}

impl Incidence {
    fn new(
        n_rows: usize,
        edges: &[(usize, usize, Scalar)],
        endpoint: impl Fn(&(usize, usize, Scalar)) -> usize,
    ) -> Self {
        let mut offsets = vec![0usize; n_rows + 1];
        for edge in edges {
            offsets[endpoint(edge) + 1] += 1;
        }
        for point in 0..n_rows {
            offsets[point + 1] += offsets[point];
        }

        if edges.windows(2).all(|w| endpoint(&w[0]) <= endpoint(&w[1])) {
            return Self::Sorted(offsets);
        }

        let mut cursor = offsets.clone();
        let mut indexed = vec![0u32; edges.len()];
        for (position, edge) in edges.iter().enumerate() {
            let slot = &mut cursor[endpoint(edge)];
            indexed[*slot] = position as u32;
            *slot += 1;
        }
        Self::Indexed {
            offsets,
            edges: indexed,
        }
    }

    /// The ids of the edges `point` is this endpoint of, ascending.
    fn of(&self, point: usize) -> Incident<'_> {
        match self {
            Self::Sorted(offsets) => Incident::Run(offsets[point]..offsets[point + 1]),
            Self::Indexed { offsets, edges } => {
                Incident::Listed(edges[offsets[point]..offsets[point + 1]].iter())
            }
        }
    }
}

/// The iterator [`Incidence::of`] returns — an enum rather than a boxed trait
/// object, which would be one allocation per point per epoch.
enum Incident<'a> {
    Run(std::ops::Range<usize>),
    Listed(std::slice::Iter<'a, u32>),
}

impl Iterator for Incident<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        match self {
            Self::Run(range) => range.next(),
            Self::Listed(ids) => ids.next().map(|&edge| edge as usize),
        }
    }
}

/// A uniform point index from `(seed, epoch, edge, draw)` alone.
///
/// SplitMix64's finaliser over a mix of the four, then Lemire's multiply-shift
/// onto `0..n_rows`. Stateless, so a draw is the same whichever thread makes it
/// and whatever was drawn before; the bias of the multiply-shift is below
/// `n_rows / 2^64`, nothing next to the sampling noise it feeds.
fn negative_sample(seed: u64, epoch: usize, edge: usize, draw: usize, n_rows: usize) -> usize {
    let mut x = seed
        ^ (epoch as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (edge as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (draw as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    ((x as u128 * n_rows as u128) >> 64) as usize
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

    /// The point of computing each epoch from a frozen layout: the answer may
    /// not depend on how many threads computed it.
    #[test]
    fn the_layout_does_not_depend_on_the_thread_count() {
        let n = 300;
        let start: Vec<Scalar> = (0..n * 2)
            .map(|i| (i as Scalar * 0.37).sin() * 10.0)
            .collect();
        let edges: Vec<(usize, usize, Scalar)> = (0..n)
            .flat_map(|i| {
                [1, 7, 31]
                    .into_iter()
                    .map(move |step| (i, (i + step) % n, 0.2 + (i % 5) as Scalar * 0.2))
            })
            .map(|(a, b, w)| (a.min(b), a.max(b), w))
            .collect();
        let mut edges = edges;
        edges.sort_by_key(|&(a, b, _)| (a, b));

        let layout_on = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let mut embedding = start.clone();
            pool.install(|| {
                optimize_layout(
                    &mut embedding,
                    n,
                    2,
                    &edges,
                    60,
                    1.577,
                    0.895,
                    1.0,
                    5,
                    1.0,
                    3,
                )
            });
            embedding
        };

        let one = layout_on(1);
        assert_eq!(one, layout_on(4));
        assert_eq!(one, layout_on(13));
    }

    /// Edges in no particular order are indexed rather than assumed sorted.
    #[test]
    fn unsorted_edges_are_laid_out_too() {
        let mut embedding: Vec<Scalar> = vec![-5.0, 0.0, 5.0, 0.0, 0.0, 5.0, 0.0, -5.0];
        let edges = vec![(2, 3, 1.0), (0, 1, 1.0), (1, 3, 0.5)];
        let before = distance(&embedding, 2, 2, 3);

        optimize_layout(
            &mut embedding,
            4,
            2,
            &edges,
            100,
            1.577,
            0.895,
            1.0,
            0,
            1.0,
            0,
        );

        assert!(embedding.iter().all(|v| v.is_finite()));
        assert!(distance(&embedding, 2, 2, 3) < before);
    }

    #[test]
    fn negative_samples_cover_the_range_and_nothing_beyond() {
        let n_rows = 7;
        let mut seen = [false; 7];
        for draw in 0..500 {
            let other = negative_sample(42, 3, 11, draw, n_rows);
            assert!(other < n_rows);
            seen[other] = true;
        }
        assert!(seen.iter().all(|&s| s));
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
