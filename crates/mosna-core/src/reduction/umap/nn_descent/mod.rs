//! Approximate nearest neighbours by descent, as umap-learn computes them.
//!
//! # Why the reduction may not use the exact search
//!
//! [`fn@crate::reduction::umap::knn_graph`] answers the neighbour question
//! exactly, and for a cohort that fits it, it is the better answer. It is not
//! the answer umap-learn gives. `UMAP.fit` computes the full distance matrix
//! only while the data has fewer than 4096 rows; past that it hands the problem
//! to `pynndescent.NNDescent`, which returns an approximation. The niches a run
//! ends up with are the niches of *that* graph — including its mistakes.
//!
//! So on the cohorts this pipeline is pointed at, hundreds of thousands of
//! cells, the reference implementation never once ran the search this port had
//! been running. Comparing the two was comparing an exact answer against an
//! approximate one and reading the difference as a porting error.
//!
//! # What this module is, and is not
//!
//! It is a port of the algorithm: the random projection forest that seeds the
//! graph, the neighbour-of-a-neighbour join that improves it, the generator
//! that drives both, and umap-learn's parameter choices. Given the same seed it
//! returns the same graph, on any machine and any number of cores.
//!
//! It is not a port of the reference's *numbers*. umap-learn is called with
//! `random_state=None`, so pynndescent seeds itself from the OS and partitions
//! its work by the core count of whatever machine it is on: two Python runs on
//! the same input give two different graphs. There is nothing to match
//! bit-for-bit, and a port that chased it would be chasing noise. What can be
//! matched is the distribution — the same kind of graph, with the same kind of
//! error, in the same places.
//!
//! # What is left out
//!
//! Only the neighbour graph is built. pynndescent goes on to prune and diversify
//! that graph into a *search index*, so later queries can be routed through it;
//! `nearest_neighbors` returns the index alongside the graph and umap keeps it
//! for `transform`. This pipeline reduces one matrix and never transforms
//! another, so the index is never built.

pub mod heap;
pub mod rng;
pub mod rp_tree;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use crate::reduction::umap::knn_graph::{knn_graph, KnnGraph};
use crate::reduction::umap::metric::Metric;
use crate::reduction::umap::Scalar;
use heap::{checked_push, Heap, EMPTY};
use rng::TauRand;

/// The size at which umap-learn stops computing every distance.
///
/// `UMAP.fit` takes the exact route while `X.shape[0] < 4096` and the
/// approximate one above it. The threshold is umap-learn's, reproduced so that
/// a cohort lands on the same side of it in both implementations.
pub const EXACT_MAX_ROWS: usize = 4096;

/// Candidates each point keeps per iteration.
///
/// pynndescent would default this to `min(60, n_neighbors)`; umap-learn passes
/// 60 explicitly, and that is what a reduction actually runs with.
const MAX_CANDIDATES: usize = 60;

/// The convergence threshold: stop once an iteration improves fewer than
/// `delta * k * n` entries.
const DELTA: f64 = 0.001;

/// Points whose candidates are joined before the thresholds are re-read.
///
/// pynndescent's `block_size`. Improvements found in one block tighten the
/// thresholds the next block filters against, so the blocking is not only about
/// bounding memory — it is part of the result.
const VERTEX_BLOCK: usize = 16_384;

/// How many parts the points are cut into for the parallel phases.
///
/// # Why a constant and not the core count
///
/// pynndescent uses `numba.get_num_threads()` here. The number decides which
/// points a worker owns, and therefore which random stream supplies their
/// candidate priorities and in which order their heaps are updated — so a
/// pynndescent run on eight cores and the same run on thirty-two do not return
/// the same graph. Fixing it makes the search reproducible, at the cost of
/// parallelism bounded by this constant rather than by the machine. Sixteen is
/// the count the reference was most often observed running with.
const N_BLOCKS: usize = 16;

/// An improvement found for a pair: `(p, q, distance)`.
///
/// Held as a proper index pair. pynndescent packs these into a `float32` array,
/// which silently rounds vertex ids past sixteen million — a cohort of twelve
/// million cells is within a factor of two of that.
type Update = (u32, u32, Scalar);

/// The `k` nearest neighbours of every row, by the route umap-learn would take.
///
/// Exact below [`EXACT_MAX_ROWS`], approximate above it. This is the function
/// the reduction calls; the clustering graph has its own entry point and its
/// own threshold, [`fn@cluster_neighbours`], because the two thresholds answer
/// different questions.
pub fn umap_neighbours(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
    seed: u64,
) -> KnnGraph {
    neighbours_below(data, n_rows, n_features, k, metric, seed, EXACT_MAX_ROWS)
}

/// Search exactly while there are fewer than `exact_below` rows, approximately
/// from there.
///
/// The two entry points differ only in that number, and it is worth their not
/// being able to drift apart: whichever route a caller lands on, it must be the
/// same routine, returning neighbours in the same order under the same
/// tie-break.
fn neighbours_below(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
    seed: u64,
    exact_below: usize,
) -> KnnGraph {
    if n_rows < exact_below {
        knn_graph(data, n_rows, n_features, k, metric)
    } else {
        nn_descent(data, n_rows, n_features, k, metric, seed)
    }
}

/// The size past which the *clustering* graph stops being searched exactly.
///
/// # Why this is not [`EXACT_MAX_ROWS`]
///
/// That threshold is umap-learn's: it is where the reference itself stops
/// computing every distance, so reproducing it is what keeps a cohort on the
/// same side of the same decision in both implementations. This one reproduces
/// nothing. `tysserand.pairs_from_knn` builds its neighbours with a scikit-learn
/// `KDTree`, which is exact at every size, so above this threshold the port
/// answers a question the reference answers exactly — and answers it
/// approximately. That is a deliberate departure, and it is the only one in
/// this stage.
///
/// # Why it is taken anyway
///
/// [`fn@crate::reduction::umap::knn_graph`] is exact and quadratic. It escapes
/// the quadratic by grouping identical rows, which is why it can search the NAS
/// feature table of a whole cohort — a Delaunay neighbourhood of six or seven
/// cells drawn from a handful of phenotypes repeats itself endlessly, and four
/// hundred thousand cells hold under a hundred thousand distinct rows. The
/// clustering graph is built on the *embedding*, and no two points of an
/// embedding coincide: the grouping finds nothing to group, gives up, and the
/// search is `O(n^2 d)` with nothing to blunt it. This is the ceiling
/// `knn_graph`'s own documentation names and offers no fallback for.
///
/// # Why nearest-neighbour descent and not a spatial tree
///
/// A k-d tree would be the better answer here and would match the reference
/// exactly: the embedding is two or three dimensions wide, which is where a
/// tree is at its best. It is the better answer only there. With
/// `reducer_type: none` the clusterer receives the NAS features themselves,
/// thirty columns and more, and a tree in thirty dimensions degenerates into
/// the scan it was meant to replace. The descent is indifferent to the width,
/// so one route serves both configurations. A tree remains worth adding for the
/// reduced case, as an exact fast path rather than a replacement for this.
///
/// # Where the value comes from
///
/// The two routes were timed against each other on rows drawn at random, which
/// is the shape this faces: nothing repeats, so the exact search gets no help
/// from its grouping. Sixteen cores, `k = 20`, best of three.
///
/// | rows | exact, 2-D | descent, 2-D | exact, 30-D | descent, 30-D |
/// |---:|---:|---:|---:|---:|
/// | 5 000 | 15 ms | 42 ms | 78 ms | 83 ms |
/// | 10 000 | 59 ms | 79 ms | 372 ms | 208 ms |
/// | 20 000 | 225 ms | 209 ms | 1 558 ms | 2 476 ms |
/// | 40 000 | 1 216 ms | 346 ms | 4 959 ms | 534 ms |
/// | 80 000 | 4 361 ms | 745 ms | 16 321 ms | 1 187 ms |
///
/// The exact search is quadratic and the descent is not, so they cross, and the
/// crossing is around twenty thousand rows — earlier in thirty dimensions,
/// later in two. Below it the exact route is both faster *and* the one the
/// reference takes, so there is nothing to trade; the threshold is placed at
/// the crossing rather than beyond it because past that point exactness stops
/// being free and starts being paid for out of the run's time budget.
///
/// The recall that buys is 1.0000 in two dimensions at every size measured —
/// on an embedding the descent simply finds the exact neighbours — and 0.92 to
/// 0.98 in thirty, which is the `reducer_type: none` case.
pub const CLUSTER_EXACT_MAX_ROWS: usize = 20_000;

/// The clustering graph must never give up on the exact search before the
/// reduction does: the reduction's threshold is the reference's, this one is a
/// cost ceiling, and a cost ceiling below it would mean approximating a search
/// that umap-learn itself would still have run exactly.
const _: () = assert!(CLUSTER_EXACT_MAX_ROWS > EXACT_MAX_ROWS);

/// The `k` nearest neighbours of every row, for the graph a clusterer partitions.
///
/// Exact below [`CLUSTER_EXACT_MAX_ROWS`], approximate above it. Callers that
/// want the reduction's threshold instead want [`fn@umap_neighbours`].
///
/// Both routes return neighbours in the same order under the same tie-break, so
/// a cohort that crosses the threshold does not change the *kind* of answer it
/// gets, only its exactness.
pub fn cluster_neighbours(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
    seed: u64,
) -> KnnGraph {
    neighbours_below(
        data,
        n_rows,
        n_features,
        k,
        metric,
        seed,
        CLUSTER_EXACT_MAX_ROWS + 1,
    )
}

/// Approximate the `k` nearest neighbours of every row by nearest-neighbour
/// descent.
///
/// The result has the same shape as the exact search's: `k` neighbours per
/// point, nearest first, never the point itself.
///
/// # The extra slot
///
/// The descent is run with `k + 1` slots per point, and the point itself is
/// dropped from its own row at the end. That is not a correction — it is how
/// pynndescent's graph is shaped. A point is compared against itself while its
/// candidates are joined, the resulting zero-distance pair is pushed like any
/// other, and every row of `NNDescent.neighbor_graph` therefore opens with the
/// point itself. umap-learn then drops that edge by giving it a membership
/// strength of zero, so a Python run asking for `n_neighbors` neighbours is
/// really working with `n_neighbors - 1` of them. Here the slot is added back
/// so that `k` means `k` real neighbours, as it does everywhere else in this
/// crate.
pub fn nn_descent(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
    seed: u64,
) -> KnnGraph {
    let k = k.min(n_rows.saturating_sub(1));
    if n_rows == 0 || k == 0 || n_rows <= 2 {
        return knn_graph(data, n_rows, n_features, k, metric);
    }

    let slots = (k + 1).min(n_rows);

    // The descent's own stream is drawn before the forest's, as `NNDescent`
    // draws `rng_state` before `make_forest` takes one state per tree.
    let mut source = ChaCha8Rng::seed_from_u64(seed);
    let mut rng = TauRand::draw(&mut source);

    let leaves = rp_tree::leaf_array(
        data,
        n_rows,
        n_features,
        leaf_size(slots),
        n_trees(n_rows),
        metric,
        &mut source,
    );

    let mut graph = Heap::new(n_rows, slots);
    init_from_leaves(&mut graph, &leaves, data, n_features, metric);
    init_random(&mut graph, data, n_features, metric, &mut rng);

    let mut candidates = Candidates::new(n_rows, MAX_CANDIDATES);
    let converged = (DELTA * slots as f64 * n_rows as f64) as usize;

    for _ in 0..n_iters(n_rows) {
        build_candidates(&mut graph, &mut candidates, &rng);
        let changes = join_candidates(&mut graph, &candidates, data, n_features, metric);
        if changes <= converged {
            break;
        }
    }

    finish(&graph, data, n_rows, n_features, k, metric)
}

/// Trees in the forest, by umap-learn's rule.
fn n_trees(n_rows: usize) -> usize {
    64.min(5 + ((n_rows as f64).sqrt() / 20.0).round() as usize)
}

/// Descent iterations, by umap-learn's rule. The loop usually stops well before
/// this on its own.
fn n_iters(n_rows: usize) -> usize {
    5.max((n_rows as f64).log2().round() as usize)
}

/// Points per leaf, by pynndescent's rule when `leaf_size` is left unset.
fn leaf_size(k: usize) -> usize {
    60.max(256.min(5 * k))
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

/// Offer every pair within every leaf to the two points it joins.
///
/// Leaves are joined in blocks, and the thresholds a block filters against are
/// read fresh: a pair found in one block raises the bar for the next.
fn init_from_leaves(
    graph: &mut Heap,
    leaves: &[Vec<u32>],
    data: &[Scalar],
    n_features: usize,
    metric: Metric,
) {
    /// Leaves joined before the thresholds are re-read, pynndescent's
    /// `n_threads * 64`.
    const LEAF_BLOCK: usize = N_BLOCKS * 64;

    for block in leaves.chunks(LEAF_BLOCK) {
        let updates: Vec<Update> = block
            .par_iter()
            .map(|leaf| {
                let mut found = Vec::new();
                for (at, &p) in leaf.iter().enumerate() {
                    let row = &data[p as usize * n_features..(p as usize + 1) * n_features];
                    let threshold_p = graph.threshold(p as usize);
                    for &q in &leaf[at + 1..] {
                        let other = &data[q as usize * n_features..(q as usize + 1) * n_features];
                        let d = metric.rank_distance(row, other);
                        if d < threshold_p.max(graph.threshold(q as usize)) {
                            found.push((p, q, d));
                        }
                    }
                }
                found
            })
            .reduce(Vec::new, |mut all, mut part| {
                all.append(&mut part);
                all
            });

        apply_updates(graph, &updates);
    }
}

/// Fill whatever slots the forest left empty with random points.
///
/// Sequential, and on the descent's own stream, as pynndescent runs it: the
/// draws are consumed in point order, and the next phase continues from where
/// this one left the state.
fn init_random(
    graph: &mut Heap,
    data: &[Scalar],
    n_features: usize,
    metric: Metric,
    rng: &mut TauRand,
) {
    let n_rows = graph.n_rows();
    let k = graph.k;

    for i in 0..n_rows {
        if graph.indices[i * k] != EMPTY {
            continue;
        }
        let filled = graph.row(i).1.iter().filter(|&&n| n != EMPTY).count();
        let row = &data[i * n_features..(i + 1) * n_features];

        for _ in 0..k - filled {
            let candidate = (rng.next_int().wrapping_abs() as usize) % n_rows;
            let other = &data[candidate * n_features..(candidate + 1) * n_features];
            let d = metric.rank_distance(other, row);
            graph.push(i, d, candidate as i32, 1);
        }
    }
}

// ---------------------------------------------------------------------------
// The descent
// ---------------------------------------------------------------------------

/// The two candidate sets each point carries into an iteration.
///
/// `new` holds neighbours found since the last join, `old` those already joined
/// against. Joining new against new and new against old, but never old against
/// old, is what keeps the iteration from redoing work it has already done.
struct Candidates {
    max: usize,
    new_indices: Vec<i32>,
    new_priority: Vec<Scalar>,
    old_indices: Vec<i32>,
    old_priority: Vec<Scalar>,
}

impl Candidates {
    fn new(n_rows: usize, max: usize) -> Self {
        Self {
            max,
            new_indices: vec![EMPTY; n_rows * max],
            new_priority: vec![Scalar::INFINITY; n_rows * max],
            old_indices: vec![EMPTY; n_rows * max],
            old_priority: vec![Scalar::INFINITY; n_rows * max],
        }
    }

    fn clear(&mut self) {
        self.new_indices.fill(EMPTY);
        self.new_priority.fill(Scalar::INFINITY);
        self.old_indices.fill(EMPTY);
        self.old_priority.fill(Scalar::INFINITY);
    }

    fn row(&self, i: usize) -> (&[i32], &[i32]) {
        let at = i * self.max;
        (
            &self.new_indices[at..at + self.max],
            &self.old_indices[at..at + self.max],
        )
    }
}

/// Collect, for every point, the neighbours and reverse neighbours it will be
/// joined against.
///
/// # Why a point's own neighbours are not enough
///
/// Descent works because being a neighbour is nearly symmetric: if `a` is close
/// to `b`, then `b`'s neighbours are worth showing to `a` even when `a` never
/// appeared in `b`'s list. So each point collects both the points it points at
/// and the points that point at it, which is why every block has to read the
/// whole graph rather than only the rows it owns.
///
/// Past [`MAX_CANDIDATES`] the surplus is dropped by random priority, not by
/// distance — deliberately, in the reference: keeping the nearest candidates
/// would keep looking where the graph has already looked.
fn build_candidates(graph: &mut Heap, candidates: &mut Candidates, base: &TauRand) {
    candidates.clear();

    let n_rows = graph.n_rows();
    let k = graph.k;
    let max = candidates.max;
    let block = n_rows / N_BLOCKS + 1;

    {
        let indices = &graph.indices;
        let flags = &graph.flags;

        candidates
            .new_indices
            .par_chunks_mut(block * max)
            .zip(candidates.new_priority.par_chunks_mut(block * max))
            .zip(candidates.old_indices.par_chunks_mut(block * max))
            .zip(candidates.old_priority.par_chunks_mut(block * max))
            .enumerate()
            .for_each(|(b, (((new_idx, new_pri), old_idx), old_pri))| {
                let mut rng = base.stream(b as i64);
                let start = b * block;
                let end = (start + max_rows_in(b, block, n_rows)).min(n_rows);

                for i in 0..n_rows {
                    for j in 0..k {
                        let candidate = indices[i * k + j];
                        if candidate == EMPTY {
                            continue;
                        }
                        let candidate = candidate as usize;
                        let owns_i = i >= start && i < end;
                        let owns_candidate = candidate >= start && candidate < end;
                        if !owns_i && !owns_candidate {
                            continue;
                        }

                        // One draw per pair per block, taken before either
                        // endpoint is pushed, so the two endpoints of a pair
                        // owned by the same block share a priority.
                        let priority = rng.next_unit();
                        let is_new = flags[i * k + j] != 0;
                        let (slot_idx, slot_pri): (&mut [i32], &mut [Scalar]) = if is_new {
                            (&mut *new_idx, &mut *new_pri)
                        } else {
                            (&mut *old_idx, &mut *old_pri)
                        };

                        if owns_i {
                            let at = (i - start) * max;
                            checked_push(
                                &mut slot_pri[at..at + max],
                                &mut slot_idx[at..at + max],
                                priority,
                                candidate as i32,
                            );
                        }
                        if owns_candidate {
                            let at = (candidate - start) * max;
                            checked_push(
                                &mut slot_pri[at..at + max],
                                &mut slot_idx[at..at + max],
                                priority,
                                i as i32,
                            );
                        }
                    }
                }
            });
    }

    // A neighbour that made it into the new candidates is about to be joined
    // against, so it stops being new.
    graph
        .flags
        .par_chunks_mut(k)
        .zip(graph.indices.par_chunks(k))
        .zip(candidates.new_indices.par_chunks(max))
        .for_each(|((flags, indices), new)| {
            for (j, &candidate) in indices.iter().enumerate() {
                if new.contains(&candidate) {
                    flags[j] = 0;
                }
            }
        });
}

/// How many points block `b` owns.
fn max_rows_in(b: usize, block: usize, n_rows: usize) -> usize {
    let start = b * block;
    if start >= n_rows {
        0
    } else {
        block.min(n_rows - start)
    }
}

/// Join each point's candidates against one another, and keep what improves the
/// graph. Returns how many entries changed.
fn join_candidates(
    graph: &mut Heap,
    candidates: &Candidates,
    data: &[Scalar],
    n_features: usize,
    metric: Metric,
) -> usize {
    let n_rows = graph.n_rows();
    let mut changes = 0;

    for start in (0..n_rows).step_by(VERTEX_BLOCK) {
        let end = (start + VERTEX_BLOCK).min(n_rows);

        let updates: Vec<Update> = (start..end)
            .into_par_iter()
            .map(|i| {
                let (new, old) = candidates.row(i);
                let mut found = Vec::new();

                for (j, &p) in new.iter().enumerate() {
                    if p == EMPTY {
                        continue;
                    }
                    let p = p as usize;
                    let row = &data[p * n_features..(p + 1) * n_features];
                    let threshold_p = graph.threshold(p);

                    // From `j`, not `j + 1`: the pair `(p, p)` is offered like
                    // any other, and is what puts a point in its own row.
                    for &q in &new[j..] {
                        if q == EMPTY {
                            continue;
                        }
                        let q = q as usize;
                        let other = &data[q * n_features..(q + 1) * n_features];
                        let d = metric.rank_distance(row, other);
                        if d <= threshold_p.max(graph.threshold(q)) {
                            found.push((p as u32, q as u32, d));
                        }
                    }

                    for &q in old {
                        if q == EMPTY {
                            continue;
                        }
                        let q = q as usize;
                        let other = &data[q * n_features..(q + 1) * n_features];
                        let d = metric.rank_distance(row, other);
                        if d <= threshold_p.max(graph.threshold(q)) {
                            found.push((p as u32, q as u32, d));
                        }
                    }
                }
                found
            })
            .reduce(Vec::new, |mut all, mut part| {
                all.append(&mut part);
                all
            });

        changes += apply_updates(graph, &updates);
    }

    changes
}

/// Offer every update to both of its endpoints.
///
/// A point belongs to exactly one block, so the blocks write disjoint rows and
/// need no locking. Each update is routed to the one or two blocks that own its
/// endpoints rather than every block scanning the whole list, which is the same
/// work in the same order for a fraction of the reading.
fn apply_updates(graph: &mut Heap, updates: &[Update]) -> usize {
    let n_rows = graph.n_rows();
    let k = graph.k;
    let block = n_rows / N_BLOCKS + 1;
    let n_blocks = n_rows.div_ceil(block);

    let mut routed: Vec<Vec<u32>> = vec![Vec::new(); n_blocks];
    for (at, &(p, q, _)) in updates.iter().enumerate() {
        let block_p = p as usize / block;
        routed[block_p].push(at as u32);
        let block_q = q as usize / block;
        if block_q != block_p {
            routed[block_q].push(at as u32);
        }
    }

    graph
        .priorities
        .par_chunks_mut(block * k)
        .zip(graph.indices.par_chunks_mut(block * k))
        .zip(graph.flags.par_chunks_mut(block * k))
        .zip(routed.into_par_iter())
        .enumerate()
        .map(|(b, (((priorities, indices), flags), mine))| {
            let start = b * block;
            let end = start + max_rows_in(b, block, n_rows);
            let mut changed = 0;

            for at in mine {
                let (p, q, d) = updates[at as usize];
                let (p, q) = (p as usize, q as usize);

                if p >= start && p < end {
                    let row = (p - start) * k;
                    changed += usize::from(heap::checked_flagged_push(
                        &mut priorities[row..row + k],
                        &mut indices[row..row + k],
                        &mut flags[row..row + k],
                        d,
                        q as i32,
                        1,
                    ));
                }
                if q >= start && q < end {
                    let row = (q - start) * k;
                    changed += usize::from(heap::checked_flagged_push(
                        &mut priorities[row..row + k],
                        &mut indices[row..row + k],
                        &mut flags[row..row + k],
                        d,
                        p as i32,
                        1,
                    ));
                }
            }
            changed
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Turn the heaps into a sorted neighbour graph.
///
/// Ties are broken by index, as the exact search breaks them; pynndescent's
/// `deheap_sort` leaves them in whatever order the heap held them, which is not
/// something a caller can rely on.
///
/// A row that comes up short — fewer than `k` neighbours found, which the
/// reference reports as a warning and lives with — is recomputed exactly. It
/// costs one pass over the data for that point alone, and it keeps the contract
/// the exact search sets: `k` neighbours, always.
fn finish(
    graph: &Heap,
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
) -> KnnGraph {
    let rows: Vec<(Vec<usize>, Vec<Scalar>)> = (0..n_rows)
        .into_par_iter()
        .map(|i| {
            let (priorities, indices) = graph.row(i);
            let mut entries: Vec<(Scalar, usize)> = priorities
                .iter()
                .zip(indices)
                .filter(|(_, &n)| n != EMPTY && n as usize != i)
                .map(|(&d, &n)| (d, n as usize))
                .collect();
            entries.sort_by(|a, b| a.partial_cmp(b).expect("distances are finite"));
            entries.truncate(k);

            if entries.len() < k {
                entries = exact_row(data, n_rows, n_features, i, k, metric);
            }

            let neighbours = entries.iter().map(|&(_, n)| n).collect();
            let distances = entries.iter().map(|&(d, _)| metric.from_rank(d)).collect();
            (neighbours, distances)
        })
        .collect();

    let mut result = KnnGraph {
        indices: Vec::with_capacity(n_rows),
        distances: Vec::with_capacity(n_rows),
    };
    for (indices, distances) in rows {
        result.indices.push(indices);
        result.distances.push(distances);
    }
    result
}

/// The exact neighbours of one point, with the exact search's tie-break.
fn exact_row(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    i: usize,
    k: usize,
    metric: Metric,
) -> Vec<(Scalar, usize)> {
    let row = &data[i * n_features..(i + 1) * n_features];
    let mut best: Vec<(Scalar, usize)> = Vec::with_capacity(k + 1);

    for j in 0..n_rows {
        if j == i {
            continue;
        }
        let other = &data[j * n_features..(j + 1) * n_features];
        let d = metric.rank_distance(row, other);
        let at = best
            .iter()
            .position(|&(best_d, best_j)| d < best_d || (d == best_d && j < best_j))
            .unwrap_or(best.len());
        best.insert(at, (d, j));
        best.truncate(k);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cloud of well-separated blobs, big enough to take the approximate
    /// route.
    fn blobs(n_rows: usize, n_features: usize, seed: u64) -> Vec<Scalar> {
        use rand::Rng;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        (0..n_rows)
            .flat_map(|i| {
                let centre = (i % 8) as Scalar * 20.0;
                (0..n_features)
                    .map(|_| centre + rng.gen_range(-1.0..1.0) as Scalar)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Fraction of the exact neighbours the approximation found.
    fn recall(approximate: &KnnGraph, exact: &KnnGraph) -> f64 {
        let mut found = 0usize;
        let mut total = 0usize;
        for (theirs, ours) in exact.indices.iter().zip(&approximate.indices) {
            for neighbour in theirs {
                total += 1;
                found += usize::from(ours.contains(neighbour));
            }
        }
        found as f64 / total as f64
    }

    #[test]
    fn finds_almost_all_of_the_exact_neighbours() {
        let (n_rows, n_features, k) = (5_000, 4, 15);
        let data = blobs(n_rows, n_features, 1);

        for metric in [Metric::Euclidean, Metric::Manhattan, Metric::Cosine] {
            let approximate = nn_descent(&data, n_rows, n_features, k, metric, 42);
            let exact = knn_graph(&data, n_rows, n_features, k, metric);
            let recall = recall(&approximate, &exact);
            assert!(recall > 0.9, "{metric:?} recall {recall}");
        }
    }

    #[test]
    fn the_graph_is_well_formed() {
        let (n_rows, n_features, k) = (5_000, 3, 10);
        let data = blobs(n_rows, n_features, 2);
        let graph = nn_descent(&data, n_rows, n_features, k, Metric::Euclidean, 7);

        for i in 0..n_rows {
            assert_eq!(graph.indices[i].len(), k);
            assert_eq!(graph.distances[i].len(), k);
            assert!(
                !graph.indices[i].contains(&i),
                "point {i} is its own neighbour"
            );

            let mut unique = graph.indices[i].clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), k, "point {i} has a repeated neighbour");

            assert!(graph.distances[i].windows(2).all(|w| w[0] <= w[1]));
            assert!(graph.distances[i]
                .iter()
                .all(|d| d.is_finite() && *d >= 0.0));
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_graph() {
        let (n_rows, n_features, k) = (5_000, 3, 8);
        let data = blobs(n_rows, n_features, 3);
        let first = nn_descent(&data, n_rows, n_features, k, Metric::Euclidean, 11);
        let second = nn_descent(&data, n_rows, n_features, k, Metric::Euclidean, 11);
        let other = nn_descent(&data, n_rows, n_features, k, Metric::Euclidean, 12);

        assert_eq!(first, second);
        assert_ne!(first, other);
    }

    /// A cohort of exact duplicates has no unique answer, but it must still
    /// have an answer: `k` distinct neighbours, all at distance zero.
    #[test]
    fn identical_rows_still_get_a_full_row() {
        let (n_rows, n_features, k) = (5_000, 3, 6);
        let data = vec![0.5 as Scalar; n_rows * n_features];
        let graph = nn_descent(&data, n_rows, n_features, k, Metric::Euclidean, 5);

        for i in 0..n_rows {
            assert_eq!(graph.indices[i].len(), k);
            assert!(!graph.indices[i].contains(&i));
            assert!(graph.distances[i].iter().all(|&d| d == 0.0));
        }
    }

    /// The feature table this runs on is mostly duplicate rows, and there the
    /// approximation stops being one.
    ///
    /// A cell's row is the phenotype composition of its neighbourhood, so a
    /// cohort of hundreds of thousands of cells holds a few thousand distinct
    /// rows and every point has thousands of others at distance zero. *Which*
    /// of them a search returns is arbitrary — comparing the identities against
    /// the exact search reports a recall near zero and means nothing — but the
    /// distances are not arbitrary, and they are what the reduction consumes.
    /// They come out exactly right.
    #[test]
    fn the_distances_are_exact_when_rows_repeat() {
        use rand::Rng;

        let (n_rows, n_phenotypes, k) = (20_000, 5, 20);
        let n_features = 2 * n_phenotypes;
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut data = Vec::with_capacity(n_rows * n_features);
        for _ in 0..n_rows {
            // Seven neighbours drawn from the phenotype vocabulary, then the
            // mean and standard deviation of each indicator over them.
            let mut counts = vec![0usize; n_phenotypes];
            for _ in 0..7 {
                counts[rng.gen_range(0..n_phenotypes)] += 1;
            }
            for &count in &counts {
                data.push(count as Scalar / 7.0);
            }
            for &count in &counts {
                let mean = count as Scalar / 7.0;
                data.push((mean * (1.0 - mean)).sqrt());
            }
        }

        let approximate = nn_descent(&data, n_rows, n_features, k, Metric::Manhattan, 42);
        let exact = knn_graph(&data, n_rows, n_features, k, Metric::Manhattan);
        assert_eq!(approximate.distances, exact.distances);
    }

    #[test]
    fn the_dispatcher_follows_umap_learns_threshold() {
        let n_features = 2;
        let k = 5;

        // Just under the threshold: exact, and identical to the exact search.
        let small = EXACT_MAX_ROWS - 1;
        let data = blobs(small, n_features, 4);
        assert_eq!(
            umap_neighbours(&data, small, n_features, k, Metric::Euclidean, 1),
            knn_graph(&data, small, n_features, k, Metric::Euclidean)
        );

        // At the threshold: the approximate route, which on separated blobs
        // still agrees with the exact one almost everywhere.
        let large = EXACT_MAX_ROWS;
        let data = blobs(large, n_features, 4);
        let exact = knn_graph(&data, large, n_features, k, Metric::Euclidean);
        let approximate = umap_neighbours(&data, large, n_features, k, Metric::Euclidean, 1);
        assert!(recall(&approximate, &exact) > 0.9);
    }

    /// The switch itself, at a threshold small enough to run both sides of.
    #[test]
    fn the_route_turns_on_the_threshold_it_is_given() {
        let (n_features, k, threshold) = (2, 5, 200);

        // One row short of the threshold: the exact search, to the letter.
        let small = threshold - 1;
        let data = blobs(small, n_features, 7);
        assert_eq!(
            neighbours_below(&data, small, n_features, k, Metric::Euclidean, 1, threshold),
            knn_graph(&data, small, n_features, k, Metric::Euclidean)
        );

        // At the threshold: the descent, which on separated blobs still finds
        // almost every neighbour the exact search does.
        let data = blobs(threshold, n_features, 7);
        let exact = knn_graph(&data, threshold, n_features, k, Metric::Euclidean);
        let approximate = neighbours_below(
            &data,
            threshold,
            n_features,
            k,
            Metric::Euclidean,
            1,
            threshold,
        );
        assert!(recall(&approximate, &exact) > 0.9);
    }

    /// The clustering graph keeps the exact search far longer than the
    /// reduction does, and for a different reason; if the two constants ever
    /// converge it should be because someone decided so.
    #[test]
    fn the_clustering_threshold_is_its_own() {
        assert_eq!(EXACT_MAX_ROWS, 4_096);
        assert_eq!(CLUSTER_EXACT_MAX_ROWS, 20_000);

        // A cohort sitting between the two thresholds: the reduction has
        // already given up on the exact search here, the clustering has not.
        let (n, n_features, k) = (EXACT_MAX_ROWS, 2, 5);
        let data = blobs(n, n_features, 9);
        assert_eq!(
            cluster_neighbours(&data, n, n_features, k, Metric::Euclidean, 1),
            knn_graph(&data, n, n_features, k, Metric::Euclidean)
        );

        // And being exact, it cannot depend on the seed — which the descent,
        // whatever it returns on any particular data, always can.
        assert_eq!(
            cluster_neighbours(&data, n, n_features, k, Metric::Euclidean, 1),
            cluster_neighbours(&data, n, n_features, k, Metric::Euclidean, 12_345)
        );
    }

    #[test]
    fn the_parameters_match_umap_learn() {
        // n_trees = min(64, 5 + round(sqrt(n) / 20))
        assert_eq!(n_trees(4_096), 8);
        assert_eq!(n_trees(955_706), 54);
        assert_eq!(n_trees(100_000_000), 64);

        // n_iters = max(5, round(log2(n)))
        assert_eq!(n_iters(4_096), 12);
        assert_eq!(n_iters(955_706), 20);
        assert_eq!(n_iters(8), 5);

        // leaf_size = max(60, min(256, 5 * k))
        assert_eq!(leaf_size(10), 60);
        assert_eq!(leaf_size(21), 105);
        assert_eq!(leaf_size(100), 256);
    }
}
