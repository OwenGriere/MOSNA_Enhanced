//! Exact k-nearest-neighbour search.

use crate::reduction::umap::Scalar;

use rayon::prelude::*;

use crate::reduction::umap::metric::Metric;

/// The `k` nearest neighbours of every point, sorted by increasing distance.
#[derive(Debug, Clone, PartialEq)]
pub struct KnnGraph {
    /// `indices[i]` are the neighbours of point `i`, nearest first.
    pub indices: Vec<Vec<usize>>,
    /// `distances[i]` are the matching distances, ascending.
    pub distances: Vec<Vec<Scalar>>,
}

impl KnnGraph {
    /// Number of points.
    pub fn n_rows(&self) -> usize {
        self.indices.len()
    }

    /// Neighbours per point, or 0 for an empty graph.
    pub fn k(&self) -> usize {
        self.indices.first().map(Vec::len).unwrap_or(0)
    }
}

/// Find the `k` nearest neighbours of every row of `data`.
///
/// `k` is clamped to `n_rows - 1`: a point is never its own neighbour, so there
/// are only that many candidates.
///
/// This is exact. Two routes reach the same answer, and which one runs is
/// decided by the data alone:
///
/// * `knn_exhaustive`, comparing every pair, at `O(n^2 d)`;
/// * `knn_by_class`, comparing every pair of *distinct* rows, at
///   `O(m^2 d + n k)` for `m` distinct rows.
///
/// Both are private: the route is an implementation detail, and the answer is
/// the same either way.
///
/// # Why distinct rows are so much rarer than rows
///
/// The matrix this is called on is the NAS feature table: for each cell, the
/// mean and standard deviation of one-hot phenotype indicators over its
/// neighbourhood. A cell's row is therefore a function of one thing only — the
/// multiset of phenotypes around it. Two cells at opposite ends of the cohort
/// whose neighbourhoods hold the same phenotypes in the same proportions have
/// not merely close rows, they have *bit-identical* rows.
///
/// A Delaunay neighbourhood holds seven cells or so, drawn from a handful of
/// phenotypes, so the number of distinct multisets is small and bounded — while
/// the number of cells is not. Measured on the cohorts this was written for:
///
/// | phenotypes | cells | distinct rows | |
/// |---|---|---|---|
/// | 4 | 287 070 | 1 032 | 278x |
/// | 5 | 727 418 | 5 173 | 141x |
/// | 16 | 156 815 | 55 439 | 2.8x |
///
/// and the distinct count flattens as cells are added, because the vocabulary
/// of neighbourhoods is exhausted long before the cohort is. Since the search
/// is quadratic, a 141-fold reduction in rows is a twenty-thousand-fold
/// reduction in work.
///
/// # The remaining ceiling
///
/// This removes a constant, not the exponent: cohorts whose rows are genuinely
/// all distinct — continuous attributes rather than one-hot phenotypes — still
/// pay `O(n^2 d)`, and no approximate index is offered as a fallback. What was
/// hours for a one-hot cohort is now seconds; what is hours for a continuous
/// one still is.
pub fn knn_graph(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
) -> KnnGraph {
    let k = k.min(n_rows.saturating_sub(1));
    if n_rows == 0 || k == 0 {
        return KnnGraph {
            indices: vec![Vec::new(); n_rows],
            distances: vec![Vec::new(); n_rows],
        };
    }

    match RowClasses::build(data, n_rows, n_features) {
        Some(classes) => knn_by_class(&classes, n_features, k, metric),
        None => knn_exhaustive(data, n_rows, n_features, k, metric),
    }
}

/// Compare every pair of rows.
fn knn_exhaustive(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    k: usize,
    metric: Metric,
) -> KnnGraph {
    // Cosine norms depend on one row each, not on the pair, so they are taken
    // once here instead of `n_rows` times inside the inner loop below.
    let norms: Vec<Scalar> = if metric == Metric::Cosine {
        (0..n_rows)
            .into_par_iter()
            .map(|i| Metric::cosine_norm(&data[i * n_features..(i + 1) * n_features]))
            .collect()
    } else {
        Vec::new()
    };
    let prunable = metric.is_additive();
    let tile = query_tile(n_features);

    let rows: Vec<(Vec<usize>, Vec<Scalar>)> = (0..n_rows.div_ceil(tile))
        .into_par_iter()
        .flat_map(|block| {
            let start = block * tile;
            let end = (start + tile).min(n_rows);

            // A bounded insertion list beats a heap here: `k` is small (tens),
            // so the linear insert is cheaper than heap bookkeeping, and it
            // keeps the result sorted for free. One per query row of the tile.
            let mut best: Vec<Vec<(Scalar, usize)>> =
                (start..end).map(|_| Vec::with_capacity(k + 1)).collect();
            let mut worst = vec![Scalar::INFINITY; end - start];

            // The candidate is the outer loop and the queries the inner one, so
            // `other` is read from memory once for the whole tile instead of
            // once per query row. Each query still sees every candidate in
            // ascending `j`, which is what fixes the tie-breaking below.
            for j in 0..n_rows {
                let other = &data[j * n_features..(j + 1) * n_features];
                for (slot, i) in (start..end).enumerate() {
                    if j == i {
                        continue;
                    }
                    let point = &data[i * n_features..(i + 1) * n_features];
                    let full = best[slot].len() == k;
                    // Once `k` neighbours are held, anything at or beyond
                    // `worst` is discarded — so the distance only has to be
                    // computed accurately enough to establish that, and an
                    // additive metric establishes it from a prefix of the
                    // vector. Retained candidates are computed in full, and
                    // identically.
                    let rank = if prunable && full {
                        metric.rank_distance_bounded(point, other, worst[slot])
                    } else if metric == Metric::Cosine {
                        Metric::cosine_with_norms(point, other, norms[i], norms[j])
                    } else {
                        metric.rank_distance(point, other)
                    };
                    if full && rank >= worst[slot] {
                        continue;
                    }
                    // Ties keep the lower index, matching a stable sort by
                    // (distance, index), so the graph is reproducible.
                    let list = &mut best[slot];
                    let position = list
                        .iter()
                        .position(|&(d, idx)| rank < d || (rank == d && j < idx))
                        .unwrap_or(list.len());
                    list.insert(position, (rank, j));
                    list.truncate(k);
                    worst[slot] = list.last().map(|&(d, _)| d).unwrap_or(Scalar::INFINITY);
                }
            }

            best.into_iter()
                .map(|list| {
                    let indices = list.iter().map(|&(_, idx)| idx).collect();
                    let distances = list.iter().map(|&(d, _)| metric.from_rank(d)).collect();
                    (indices, distances)
                })
                .collect::<Vec<_>>()
        })
        .collect();

    let mut graph = KnnGraph {
        indices: Vec::with_capacity(n_rows),
        distances: Vec::with_capacity(n_rows),
    };
    for (indices, distances) in rows {
        graph.indices.push(indices);
        graph.distances.push(distances);
    }
    graph
}

/// Rows grouped by exact equality, in first-seen order.
///
/// # Why bit patterns rather than numeric equality
///
/// Grouping on `to_bits` puts `0.0` and `-0.0` in different classes even though
/// they compare equal. That costs a redundant class and changes no answer: the
/// two classes come out at distance zero from one another and are merged by the
/// equal-distance rule in [`fn@knn_by_class`], exactly as the exhaustive search
/// would have interleaved their members. Erring this way is deliberate — the
/// grouping may split what could have been merged, never merge what differs.
struct RowClasses {
    /// Class of each row.
    class_of: Vec<u32>,
    /// Rows of each class, ascending. Class ids follow first-seen order, so
    /// `members[c][0]` increases with `c`.
    members: Vec<Vec<u32>>,
    /// One representative row per class, row-major.
    representatives: Vec<Scalar>,
}

impl RowClasses {
    /// Group the rows, or `None` when too few repeat to pay for the grouping.
    fn build(data: &[Scalar], n_rows: usize, n_features: usize) -> Option<Self> {
        /// Below this much duplication the class machinery costs more than the
        /// quadratic search it saves.
        const WORTH_IT: Scalar = 0.9;
        /// How often that verdict is revisited while the rows are read.
        const PROBE_EVERY: usize = 1 << 16;

        let mut buckets: std::collections::HashMap<u64, Vec<u32>> = Default::default();
        let mut class_of = vec![0u32; n_rows];
        let mut first_row: Vec<u32> = Vec::new();

        for row in 0..n_rows {
            let values = &data[row * n_features..(row + 1) * n_features];
            let bucket = buckets.entry(hash_row(values)).or_default();

            // A bucket holds the classes whose representative hashes here, so
            // the comparison below is what decides equality; the hash only
            // narrows the field.
            let found = bucket.iter().copied().find(|&candidate| {
                let at = first_row[candidate as usize] as usize * n_features;
                values
                    .iter()
                    .zip(&data[at..at + n_features])
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            });

            class_of[row] = match found {
                Some(class) => class,
                None => {
                    let class = first_row.len() as u32;
                    first_row.push(row as u32);
                    bucket.push(class);
                    class
                }
            };

            // Nothing repeats often enough for the second pass to be worth it.
            //
            // Judged on the rows seen so far, and re-judged periodically, so a
            // cohort of entirely distinct rows abandons the attempt after a few
            // tens of thousands of them rather than building a map of the whole
            // thing first — which for twelve million rows would be a wasted
            // half-gigabyte. Giving up costs only the exhaustive search this
            // was trying to avoid, so the verdict may be early and rough.
            let seen = row + 1;
            if (seen % PROBE_EVERY == 0 || seen == n_rows)
                && first_row.len() as Scalar > seen as Scalar * WORTH_IT
            {
                return None;
            }
        }

        let mut members = vec![Vec::new(); first_row.len()];
        for (row, &class) in class_of.iter().enumerate() {
            members[class as usize].push(row as u32);
        }

        let mut representatives = Vec::with_capacity(first_row.len() * n_features);
        for &row in &first_row {
            let at = row as usize * n_features;
            representatives.extend_from_slice(&data[at..at + n_features]);
        }

        Some(Self {
            class_of,
            members,
            representatives,
        })
    }

    fn n_classes(&self) -> usize {
        self.members.len()
    }
}

/// FNV-1a over the bit patterns of a row.
///
/// Only ever used to bucket candidates for an exact comparison, so its quality
/// costs time and never correctness.
fn hash_row(values: &[Scalar]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for value in values {
        for byte in value.to_bits().to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// Compare every pair of *distinct* rows, then expand back to every row.
///
/// # Why the nearest `k + 1` classes are enough
///
/// The exhaustive search ranks candidates by `(distance, row index)`. Class ids
/// are handed out in first-seen order, so ordering classes by
/// `(distance, class id)` orders them by `(distance, lowest member row)`.
///
/// Suppose a class `D` holds one of the `k` nearest rows of a query, but is not
/// among the `k + 1` nearest classes. Then `k + 1` classes rank strictly before
/// `D`, and each contributes at least its own lowest member — a row that ranks
/// strictly before every row of `D`. That is `k + 1` rows ahead of the one in
/// `D`, of which at most one is the query itself, so the row in `D` ranks
/// `k + 1`-th or worse. It was not among the nearest `k` after all.
fn knn_by_class(classes: &RowClasses, n_features: usize, k: usize, metric: Metric) -> KnnGraph {
    let n_classes = classes.n_classes();
    let reps = &classes.representatives;

    // The class-level search, on the same code path and the same tie-break.
    let wanted = (k + 1).min(n_classes.saturating_sub(1));
    let class_graph = knn_exhaustive(reps, n_classes, n_features, wanted, metric);

    let norms: Vec<Scalar> = if metric == Metric::Cosine {
        (0..n_classes)
            .map(|c| Metric::cosine_norm(&reps[c * n_features..(c + 1) * n_features]))
            .collect()
    } else {
        Vec::new()
    };
    let rank = |a: usize, b: usize| {
        let (x, y) = (
            &reps[a * n_features..(a + 1) * n_features],
            &reps[b * n_features..(b + 1) * n_features],
        );
        match metric {
            // The same expression the search itself evaluated, so a class
            // compared with itself yields whatever that yields — which for
            // cosine is not necessarily zero, and must not be assumed to be.
            Metric::Cosine => Metric::cosine_with_norms(x, y, norms[a], norms[b]),
            _ => metric.rank_distance(x, y),
        }
    };

    // Per class, the candidate classes in `(rank, class id)` order — its own
    // class merged into the neighbours at the position its self-distance earns.
    let plans: Vec<Vec<(Scalar, u32)>> = (0..n_classes)
        .into_par_iter()
        .map(|c| {
            let mut plan: Vec<(Scalar, u32)> = Vec::with_capacity(wanted + 1);
            let own = (rank(c, c), c as u32);
            let mut placed = false;
            for &d in &class_graph.indices[c] {
                let entry = (rank(c, d), d as u32);
                if !placed && (own.0 < entry.0 || (own.0 == entry.0 && own.1 < entry.1)) {
                    plan.push(own);
                    placed = true;
                }
                plan.push(entry);
            }
            if !placed {
                plan.push(own);
            }
            plan
        })
        .collect();

    let rows: Vec<(Vec<usize>, Vec<Scalar>)> = classes
        .class_of
        .par_iter()
        .enumerate()
        .map(|(row, &class)| {
            let plan = &plans[class as usize];
            let mut indices = Vec::with_capacity(k);
            let mut distances = Vec::with_capacity(k);

            let mut at = 0usize;
            while at < plan.len() && indices.len() < k {
                // Classes at the same distance are one pool: their members
                // interleave by row index, exactly as the exhaustive search
                // would have met them.
                let mut end = at + 1;
                while end < plan.len() && plan[end].0 == plan[at].0 {
                    end += 1;
                }
                let group_rank = plan[at].0;
                let mut cursors: Vec<(usize, usize)> = plan[at..end]
                    .iter()
                    .map(|&(_, c)| (c as usize, 0))
                    .collect();

                while indices.len() < k {
                    // The smallest unconsumed row index across the pool.
                    let mut pick: Option<(u32, usize)> = None;
                    for (slot, &(class, offset)) in cursors.iter().enumerate() {
                        if let Some(&candidate) = classes.members[class].get(offset) {
                            if pick.is_none_or(|(best, _)| candidate < best) {
                                pick = Some((candidate, slot));
                            }
                        }
                    }
                    let Some((candidate, slot)) = pick else { break };
                    cursors[slot].1 += 1;
                    if candidate as usize == row {
                        continue;
                    }
                    indices.push(candidate as usize);
                    distances.push(metric.from_rank(group_rank));
                }
                at = end;
            }

            (indices, distances)
        })
        .collect();

    let mut graph = KnnGraph {
        indices: Vec::with_capacity(rows.len()),
        distances: Vec::with_capacity(rows.len()),
    };
    for (indices, distances) in rows {
        graph.indices.push(indices);
        graph.distances.push(distances);
    }
    graph
}

/// How many query rows to hold against one pass over the candidates.
///
/// The exhaustive search is bound by memory bandwidth, not by arithmetic: with
/// one query row at a time the whole dataset is streamed once per row, which is
/// `n^2 * n_features * 8` bytes of traffic. Handling a tile of query rows at
/// once divides that by the tile size, and costs nothing in accuracy because
/// each query still visits every candidate in the same order.
///
/// The tile is sized so its rows stay in L1 alongside the candidate being
/// compared — 16 KiB of query data, whatever the feature count, and never wider
/// than 64 rows, past which the per-candidate bookkeeping starts to dominate.
fn query_tile(n_features: usize) -> usize {
    const BUDGET_BYTES: usize = 16 * 1024;
    let per_row = n_features.max(1) * std::mem::size_of::<Scalar>();
    (BUDGET_BYTES / per_row).clamp(1, 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points on a line at 0, 1, 2, 3, 4.
    fn line() -> Vec<Scalar> {
        (0..5).map(|i| i as Scalar).collect()
    }

    #[test]
    fn finds_the_nearest_neighbours_in_order() {
        let data = line();
        let graph = knn_graph(&data, 5, 1, 2, Metric::Euclidean);

        assert_eq!(graph.indices[0], vec![1, 2]);
        assert_eq!(graph.distances[0], vec![1.0, 2.0]);
        // Point 2 is equidistant from 1 and 3; the lower index wins.
        assert_eq!(graph.indices[2], vec![1, 3]);
    }

    #[test]
    fn a_point_is_never_its_own_neighbour() {
        let data = line();
        let graph = knn_graph(&data, 5, 1, 4, Metric::Euclidean);
        for (i, neighbours) in graph.indices.iter().enumerate() {
            assert!(!neighbours.contains(&i));
        }
    }

    #[test]
    fn k_is_clamped_to_the_available_candidates() {
        let data = line();
        let graph = knn_graph(&data, 5, 1, 99, Metric::Euclidean);
        assert!(graph.indices.iter().all(|n| n.len() == 4));
        assert_eq!(graph.k(), 4);
    }

    #[test]
    fn distances_use_the_requested_metric() {
        let data = vec![0.0, 0.0, 3.0, 4.0];
        let euclidean = knn_graph(&data, 2, 2, 1, Metric::Euclidean);
        assert_eq!(euclidean.distances[0], vec![5.0]);

        let manhattan = knn_graph(&data, 2, 2, 1, Metric::Manhattan);
        assert_eq!(manhattan.distances[0], vec![7.0]);
    }

    #[test]
    fn ties_are_broken_by_index_so_the_graph_is_reproducible() {
        // Four points at the same place.
        let data = vec![0.0; 8];
        let first = knn_graph(&data, 4, 2, 2, Metric::Euclidean);
        let second = knn_graph(&data, 4, 2, 2, Metric::Euclidean);
        assert_eq!(first, second);
        assert_eq!(first.indices[3], vec![0, 1]);
    }

    #[test]
    fn degenerate_inputs_produce_an_empty_graph() {
        let empty = knn_graph(&[], 0, 2, 3, Metric::Euclidean);
        assert_eq!(empty.n_rows(), 0);

        let single = knn_graph(&[1.0, 2.0], 1, 2, 3, Metric::Euclidean);
        assert_eq!(single.n_rows(), 1);
        assert!(single.indices[0].is_empty());
    }

    // -----------------------------------------------------------------------
    // Grouping identical rows
    //
    // The class route exists only to be faster. Every test here asks the same
    // question — does it return what comparing every pair would have returned —
    // because any answer other than "yes, exactly" makes it worthless.
    // -----------------------------------------------------------------------

    /// A table with `distinct` different rows, repeated to fill `n` rows, in an
    /// order that interleaves the classes rather than blocking them.
    fn repeated(n: usize, n_features: usize, distinct: usize) -> Vec<Scalar> {
        let mut data = Vec::with_capacity(n * n_features);
        for row in 0..n {
            // A stride coprime with `distinct` so consecutive rows differ.
            let class = (row * 7) % distinct;
            for f in 0..n_features {
                data.push(((class * 31 + f * 17) % 11) as Scalar / 4.0);
            }
        }
        data
    }

    /// The property the whole optimisation rests on.
    #[test]
    fn grouping_identical_rows_changes_no_neighbour() {
        for (n, n_features, distinct, k) in [
            (200, 6, 9, 5),
            (300, 4, 40, 12),
            (150, 3, 2, 7),
            (97, 5, 96, 4),
        ] {
            let data = repeated(n, n_features, distinct);
            for metric in [Metric::Euclidean, Metric::Manhattan, Metric::Cosine] {
                let grouped = knn_graph(&data, n, n_features, k, metric);
                let exhaustive = knn_exhaustive(&data, n, n_features, k, metric);
                assert_eq!(
                    grouped, exhaustive,
                    "{metric:?} disagreed on {n}x{n_features} with {distinct} distinct rows"
                );
            }
        }
    }

    /// The pathological shape: every row identical, so the only thing that
    /// orders the neighbours is the row index.
    #[test]
    fn a_table_of_one_repeated_row_falls_back_on_the_index() {
        let n = 50;
        let data = vec![0.25; n * 3];
        for metric in [Metric::Euclidean, Metric::Manhattan, Metric::Cosine] {
            let graph = knn_graph(&data, n, 3, 4, metric);
            assert_eq!(graph, knn_exhaustive(&data, n, 3, 4, metric), "{metric:?}");
            // Row 10's neighbours are the four lowest-numbered other rows.
            assert_eq!(graph.indices[10], vec![0, 1, 2, 3]);
            // And row 0's are the four that follow it.
            assert_eq!(graph.indices[0], vec![1, 2, 3, 4]);
        }
    }

    /// `0.0` and `-0.0` are different bit patterns and equal numbers. They may
    /// land in different classes; they may not end up in a different answer.
    #[test]
    fn signed_zero_is_grouped_conservatively() {
        let n_features = 2;
        let data = vec![
            0.0, 1.0, //
            -0.0, 1.0, //
            0.0, 1.0, //
            5.0, 1.0, //
            -0.0, 1.0,
        ];
        for metric in [Metric::Euclidean, Metric::Manhattan] {
            assert_eq!(
                knn_graph(&data, 5, n_features, 3, metric),
                knn_exhaustive(&data, 5, n_features, 3, metric),
                "{metric:?}"
            );
        }
    }

    /// Rows that all differ must not be made slower *and* must not be made
    /// wrong: the grouping gives up and the exhaustive search runs.
    #[test]
    fn a_table_with_nothing_repeated_takes_the_exhaustive_route() {
        let n = 60;
        let data: Vec<Scalar> = (0..n * 2).map(|i| i as Scalar * 1.37).collect();
        assert!(
            RowClasses::build(&data, n, 2).is_none(),
            "nothing repeats, so there is nothing to group"
        );
        assert_eq!(
            knn_graph(&data, n, 2, 5, Metric::Euclidean),
            knn_exhaustive(&data, n, 2, 5, Metric::Euclidean)
        );
    }

    /// Class ids have to follow first-seen order: the argument for visiting
    /// only `k + 1` classes reads them as a stand-in for the lowest member row.
    #[test]
    fn class_ids_follow_first_seen_order() {
        // Rows: A B A C B — three classes, first seen at rows 0, 1, 3.
        let data = vec![1.0, 1.0, 2.0, 2.0, 1.0, 1.0, 3.0, 3.0, 2.0, 2.0];
        let classes = RowClasses::build(&data, 5, 2).expect("two rows repeat");
        assert_eq!(classes.class_of, vec![0, 1, 0, 2, 1]);
        assert_eq!(classes.members[0], vec![0, 2]);
        assert_eq!(classes.members[1], vec![1, 4]);
        assert_eq!(classes.members[2], vec![3]);
        // And the lowest member rises with the class id, which is the property
        // the `k + 1` bound is proved from.
        assert!(classes
            .members
            .windows(2)
            .all(|pair| pair[0][0] < pair[1][0]));
    }

    /// `k` larger than the pool a class can supply still comes back full, by
    /// walking on into the classes behind it.
    #[test]
    fn a_class_smaller_than_k_draws_from_the_next_ones() {
        // Four classes of three rows each, k = 7.
        let data = repeated(12, 2, 4);
        let graph = knn_graph(&data, 12, 2, 7, Metric::Euclidean);
        assert!(graph.indices.iter().all(|n| n.len() == 7));
        assert_eq!(graph, knn_exhaustive(&data, 12, 2, 7, Metric::Euclidean));
    }
}
