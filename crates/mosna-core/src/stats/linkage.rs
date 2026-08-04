//! Ward hierarchical clustering — port of the
//! `scipy.cluster.hierarchy.linkage(pdist(X), method="ward")` and
//! `dendrogram(...)["leaves"]` calls in `assort_figures_heatmap.py`.

/// One agglomeration step: the two clusters merged, the distance at which they
/// merged, and the size of the result.
///
/// Matches a row of scipy's linkage matrix `[left, right, distance, count]`,
/// where a value below `n` is an original observation and a value of `n + k` is
/// the cluster formed by step `k`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Merge {
    pub left: usize,
    pub right: usize,
    pub distance: f64,
    pub count: usize,
}

/// The result of a hierarchical clustering: `n - 1` merges over `n` leaves.
#[derive(Debug, Clone, PartialEq)]
pub struct Linkage {
    pub n_leaves: usize,
    pub merges: Vec<Merge>,
}

/// Cluster `points` with Ward's method on Euclidean distances.
///
/// Uses the nearest-neighbour chain algorithm, which is `O(n²)` in time and
/// memory and produces exactly the same dendrogram as scipy's implementation
/// of the same algorithm.
///
/// Rows are the observations. The heatmap figure clusters both the images
/// (rows of the transposed matrix) and the phenotype pairs, so this is called
/// twice per figure with matrices of a few hundred rows at most.
pub fn ward_linkage(points: &[Vec<f64>]) -> Linkage {
    let n = points.len();
    if n <= 1 {
        return Linkage {
            n_leaves: n,
            merges: Vec::new(),
        };
    }

    // Ward's update rule is expressed on squared distances; the reported
    // distance is the square root, as scipy reports it.
    let mut d = vec![0.0f64; n * n];
    for i in 0..n {
        for j in (i + 1)..n {
            let dist: f64 = points[i]
                .iter()
                .zip(&points[j])
                .map(|(a, b)| (a - b) * (a - b))
                .sum();
            d[i * n + j] = dist;
            d[j * n + i] = dist;
        }
    }

    let mut size = vec![1usize; n];
    let mut active: Vec<bool> = vec![true; n];
    // Identifier of the cluster currently held in slot `i`, in scipy numbering.
    let mut label: Vec<usize> = (0..n).collect();

    let mut merges: Vec<Merge> = Vec::with_capacity(n - 1);
    let mut chain: Vec<usize> = Vec::with_capacity(n);

    for step in 0..(n - 1) {
        if chain.is_empty() {
            let start = (0..n)
                .find(|&i| active[i])
                .expect("an active cluster remains");
            chain.push(start);
        }

        // Grow the chain until it closes on a mutual nearest-neighbour pair.
        let (a, b) = loop {
            let a = *chain.last().expect("chain is never empty here");
            let mut best = usize::MAX;
            let mut best_dist = f64::INFINITY;
            for candidate in 0..n {
                if !active[candidate] || candidate == a {
                    continue;
                }
                let dist = d[a * n + candidate];
                // A strict comparison keeps the lowest index on a tie, which is
                // what scipy's scan does.
                if dist < best_dist {
                    best_dist = dist;
                    best = candidate;
                }
            }
            debug_assert!(best != usize::MAX, "at least two clusters are active");

            if chain.len() >= 2 && best == chain[chain.len() - 2] {
                chain.pop();
                chain.pop();
                break (a, best);
            }
            chain.push(best);
        };

        let (i, j) = if a < b { (a, b) } else { (b, a) };
        let merged_size = size[i] + size[j];
        merges.push(Merge {
            left: label[i].min(label[j]),
            right: label[i].max(label[j]),
            distance: d[i * n + j].max(0.0).sqrt(),
            count: merged_size,
        });

        // Lance-Williams update for Ward, on squared distances:
        //   d(I u J, K) = ((nI+nK) d(I,K) + (nJ+nK) d(J,K) - nK d(I,J))
        //                 / (nI + nJ + nK)
        let d_ij = d[i * n + j];
        for k in 0..n {
            if !active[k] || k == i || k == j {
                continue;
            }
            let n_i = size[i] as f64;
            let n_j = size[j] as f64;
            let n_k = size[k] as f64;
            let updated = ((n_i + n_k) * d[i * n + k] + (n_j + n_k) * d[j * n + k] - n_k * d_ij)
                / (n_i + n_j + n_k);
            d[i * n + k] = updated;
            d[k * n + i] = updated;
        }

        // Slot `i` now holds the merged cluster; slot `j` retires.
        active[j] = false;
        size[i] = merged_size;
        label[i] = n + step;
    }

    order_merges(n, merges)
}

/// Renumber the merges so that step `k` creates cluster `n + k` and distances
/// are non-decreasing, which is the invariant scipy's linkage matrix carries.
///
/// The chain algorithm discovers merges in an order driven by the chain, not by
/// distance. Re-sorting has to respect dependencies — a merge cannot be listed
/// before one of its children — so this repeatedly emits the cheapest merge
/// whose children are already emitted.
fn order_merges(n_leaves: usize, merges: Vec<Merge>) -> Linkage {
    let total = merges.len();
    let mut emitted: Vec<Option<usize>> = vec![None; total];
    let mut done = vec![false; total];
    let mut ordered = Vec::with_capacity(total);

    let resolved = |value: usize, emitted: &Vec<Option<usize>>| -> Option<usize> {
        if value < n_leaves {
            Some(value)
        } else {
            emitted[value - n_leaves].map(|k| n_leaves + k)
        }
    };

    for step in 0..total {
        let mut best: Option<(usize, f64)> = None;
        for (idx, merge) in merges.iter().enumerate() {
            if done[idx] {
                continue;
            }
            if resolved(merge.left, &emitted).is_none() || resolved(merge.right, &emitted).is_none()
            {
                continue;
            }
            if best.is_none_or(|(_, d)| merge.distance < d) {
                best = Some((idx, merge.distance));
            }
        }
        let (idx, _) = best.expect("the merge forest is acyclic, so one is always ready");
        let merge = merges[idx];
        let left = resolved(merge.left, &emitted).expect("checked above");
        let right = resolved(merge.right, &emitted).expect("checked above");
        ordered.push(Merge {
            left: left.min(right),
            right: left.max(right),
            distance: merge.distance,
            count: merge.count,
        });
        done[idx] = true;
        emitted[idx] = Some(step);
    }

    Linkage {
        n_leaves,
        merges: ordered,
    }
}

/// Leaf order of the dendrogram, as `dendrogram(Z)["leaves"]` returns it.
///
/// Depth-first from the root, visiting the left child of each merge before the
/// right one — scipy's behaviour with its default `count_sort=False` and
/// `distance_sort=False`.
///
/// Only the visual arrangement of the heatmap depends on this; no number does.
pub fn dendrogram_leaf_order(linkage: &Linkage) -> Vec<usize> {
    let n = linkage.n_leaves;
    if n == 0 {
        return Vec::new();
    }
    if linkage.merges.is_empty() {
        return (0..n).collect();
    }

    let mut order = Vec::with_capacity(n);
    // Iterative DFS, so a deep dendrogram cannot overflow the stack.
    let root = n + linkage.merges.len() - 1;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node < n {
            order.push(node);
            continue;
        }
        let merge = &linkage.merges[node - n];
        // Pushed right-first so the left child is popped first.
        stack.push(merge.right);
        stack.push(merge.left);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_two_points_at_their_distance() {
        let points = vec![vec![0.0], vec![3.0]];
        let linkage = ward_linkage(&points);
        assert_eq!(linkage.merges.len(), 1);
        assert_eq!(linkage.merges[0].left, 0);
        assert_eq!(linkage.merges[0].right, 1);
        assert!((linkage.merges[0].distance - 3.0).abs() < 1e-12);
        assert_eq!(linkage.merges[0].count, 2);
    }

    /// Two tight pairs, far apart: the pairs must merge before the groups do.
    #[test]
    fn recovers_an_obvious_two_group_structure() {
        let points = vec![
            vec![0.0, 0.0],
            vec![0.1, 0.0],
            vec![10.0, 0.0],
            vec![10.1, 0.0],
        ];
        let linkage = ward_linkage(&points);
        assert_eq!(linkage.merges.len(), 3);

        // The first two merges are the within-pair ones.
        let first_two: Vec<(usize, usize)> = linkage.merges[..2]
            .iter()
            .map(|m| (m.left, m.right))
            .collect();
        assert!(first_two.contains(&(0, 1)), "got {first_two:?}");
        assert!(first_two.contains(&(2, 3)), "got {first_two:?}");

        // The last merge joins the two composite clusters and is much costlier.
        let last = linkage.merges[2];
        assert_eq!(last.count, 4);
        assert!(last.distance > linkage.merges[1].distance * 10.0);
    }

    #[test]
    fn distances_are_non_decreasing() {
        let points: Vec<Vec<f64>> = (0..12)
            .map(|i| vec![(i as f64 * 1.7).sin(), (i as f64 * 0.9).cos()])
            .collect();
        let linkage = ward_linkage(&points);
        for pair in linkage.merges.windows(2) {
            assert!(
                pair[0].distance <= pair[1].distance + 1e-12,
                "linkage must be monotonic: {pair:?}"
            );
        }
    }

    #[test]
    fn every_step_creates_the_next_cluster_id() {
        let points: Vec<Vec<f64>> = (0..8).map(|i| vec![i as f64]).collect();
        let linkage = ward_linkage(&points);
        for (step, merge) in linkage.merges.iter().enumerate() {
            let new_id = linkage.n_leaves + step;
            assert!(
                merge.left < new_id && merge.right < new_id,
                "step {step} references a cluster that does not exist yet"
            );
        }
        assert_eq!(linkage.merges.last().unwrap().count, 8);
    }

    #[test]
    fn leaf_order_is_a_permutation_of_every_leaf() {
        let points: Vec<Vec<f64>> = (0..10)
            .map(|i| vec![(i as f64).sin() * 5.0, (i as f64).cos() * 5.0])
            .collect();
        let linkage = ward_linkage(&points);
        let order = dendrogram_leaf_order(&linkage);

        assert_eq!(order.len(), 10);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn leaf_order_keeps_close_points_adjacent() {
        let points = vec![vec![0.0], vec![10.0], vec![0.1], vec![10.1]];
        let linkage = ward_linkage(&points);
        let order = dendrogram_leaf_order(&linkage);
        let position = |leaf: usize| order.iter().position(|&l| l == leaf).unwrap();

        // 0 and 2 are neighbours, as are 1 and 3.
        assert_eq!(position(0).abs_diff(position(2)), 1, "order was {order:?}");
        assert_eq!(position(1).abs_diff(position(3)), 1, "order was {order:?}");
    }

    #[test]
    fn degenerate_inputs_are_handled() {
        assert!(ward_linkage(&[]).merges.is_empty());
        let single = ward_linkage(&[vec![1.0]]);
        assert!(single.merges.is_empty());
        assert_eq!(dendrogram_leaf_order(&single), vec![0]);
        assert!(dendrogram_leaf_order(&ward_linkage(&[])).is_empty());
    }
}
