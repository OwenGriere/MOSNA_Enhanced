//! How much two partitions, or two spaces, agree.
//!
//! Level 3 covers what is irreducibly stochastic: the SGD inside UMAP and the
//! local moves inside Leiden. Two runs there produce partitions that mean the
//! same thing under different names, so a label-by-label comparison says
//! nothing. These metrics say something.
//!
//! # Why the adjusted Rand index and not the raw one
//!
//! The plain Rand index counts agreeing pairs, and two *independent* random
//! partitions already agree on most pairs simply because most pairs are apart
//! in both. It therefore reads around 0.5 for partitions with nothing in
//! common. The adjusted index subtracts what chance alone would give, so
//! independence reads zero — which is the number a benchmark needs.
//!
//! # Why not the adjusted mutual information
//!
//! Its correction term needs the expected mutual information over all
//! contingency tables with the given margins, which needs a log-gamma. The
//! adjusted Rand index already provides a chance-corrected number, and the
//! plain normalised mutual information provides the information-theoretic view
//! alongside it. Adding a hand-written log-gamma to get a third number that
//! agrees with the first two would be effort spent for no extra evidence.

use std::collections::HashMap;

/// Contingency table between two labellings, plus the two sets of margins.
struct Contingency {
    joint: Vec<f64>,
    left: Vec<f64>,
    right: Vec<f64>,
    n: f64,
}

fn contingency(a: &[u32], b: &[u32]) -> Option<Contingency> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }

    let index_of = |labels: &[u32]| -> (HashMap<u32, usize>, usize) {
        let mut map = HashMap::new();
        for &label in labels {
            let next = map.len();
            map.entry(label).or_insert(next);
        }
        let size = map.len();
        (map, size)
    };

    let (left_index, rows) = index_of(a);
    let (right_index, cols) = index_of(b);

    let mut joint = vec![0.0; rows * cols];
    let mut left = vec![0.0; rows];
    let mut right = vec![0.0; cols];

    for (x, y) in a.iter().zip(b) {
        let row = left_index[x];
        let col = right_index[y];
        joint[row * cols + col] += 1.0;
        left[row] += 1.0;
        right[col] += 1.0;
    }

    Some(Contingency {
        joint,
        left,
        right,
        n: a.len() as f64,
    })
}

/// `n choose 2`, the number of pairs inside a group of `n`.
fn pairs(n: f64) -> f64 {
    n * (n - 1.0) / 2.0
}

/// The adjusted Rand index of two labellings.
///
/// `1` for identical partitions up to renaming, `0` for partitions no more
/// alike than chance, negative for partitions less alike than chance.
///
/// Returns `NaN` for empty input rather than a number that looks like an
/// answer.
pub fn adjusted_rand_index(a: &[u32], b: &[u32]) -> f64 {
    let Some(table) = contingency(a, b) else {
        return f64::NAN;
    };

    let observed: f64 = table.joint.iter().copied().map(pairs).sum();
    let left: f64 = table.left.iter().copied().map(pairs).sum();
    let right: f64 = table.right.iter().copied().map(pairs).sum();
    let total = pairs(table.n);

    if total == 0.0 {
        return f64::NAN;
    }

    let expected = left * right / total;
    let maximum = (left + right) / 2.0;

    // Both partitions in singletons, or both in one group: every partition
    // agrees, and the correction divides by zero. That is perfect agreement.
    if (maximum - expected).abs() < f64::EPSILON {
        return 1.0;
    }
    (observed - expected) / (maximum - expected)
}

/// The mutual information of two labellings, normalised by the mean of their
/// entropies.
///
/// `1` when each partition determines the other, `0` when one says nothing
/// about the other.
pub fn normalized_mutual_information(a: &[u32], b: &[u32]) -> f64 {
    let Some(table) = contingency(a, b) else {
        return f64::NAN;
    };

    let entropy = |counts: &[f64]| -> f64 {
        -counts
            .iter()
            .filter(|&&count| count > 0.0)
            .map(|&count| {
                let p = count / table.n;
                p * p.ln()
            })
            .sum::<f64>()
    };

    let left_entropy = entropy(&table.left);
    let right_entropy = entropy(&table.right);

    // A partition with a single group carries no information; the normaliser
    // is zero and so, by convention, is the score.
    if left_entropy == 0.0 || right_entropy == 0.0 {
        return 0.0;
    }

    let cols = table.right.len();
    let mut mutual = 0.0;
    for (row, &left_count) in table.left.iter().enumerate() {
        for (col, &right_count) in table.right.iter().enumerate() {
            let joint = table.joint[row * cols + col];
            if joint > 0.0 {
                let p = joint / table.n;
                mutual += p * ((joint * table.n) / (left_count * right_count)).ln();
            }
        }
    }

    (mutual / ((left_entropy + right_entropy) / 2.0)).clamp(0.0, 1.0)
}

/// The proportion of each point's `k` nearest neighbours that survive into the
/// embedding.
///
/// This is the question to ask of a projection: a rotated, shifted or rescaled
/// embedding keeps every neighbourhood and scores `1`, while a projection that
/// tore the structure apart scores near the value chance would give.
///
/// Both spaces are given as row-major slices; a point count of `n` with
/// `high_dim` and `low_dim` features respectively.
pub fn knn_overlap(
    high: &[f64],
    low: &[f64],
    n: usize,
    high_dim: usize,
    low_dim: usize,
    k: usize,
) -> f64 {
    if n < 2 {
        return f64::NAN;
    }
    // A point is not its own neighbour, so `k` cannot exceed `n - 1`.
    let k = k.min(n - 1);

    let neighbours = |data: &[f64], dim: usize, point: usize| -> Vec<usize> {
        let mut distances: Vec<(f64, usize)> = (0..n)
            .filter(|&other| other != point)
            .map(|other| {
                let d: f64 = (0..dim)
                    .map(|f| {
                        let delta = data[point * dim + f] - data[other * dim + f];
                        delta * delta
                    })
                    .sum();
                (d, other)
            })
            .collect();
        // Ties are broken by index, so the metric does not depend on the order
        // the distances happened to be computed in.
        distances.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        distances
            .into_iter()
            .take(k)
            .map(|(_, index)| index)
            .collect()
    };

    let mut kept = 0usize;
    for point in 0..n {
        let before = neighbours(high, high_dim, point);
        let after = neighbours(low, low_dim, point);
        kept += before.iter().filter(|index| after.contains(index)).count();
    }

    kept as f64 / (n * k) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contingency_table_counts_every_cell() {
        let table = contingency(&[0, 0, 1], &[1, 1, 0]).unwrap();
        assert_eq!(table.n, 3.0);
        assert_eq!(table.joint.iter().sum::<f64>(), 3.0);
        assert_eq!(table.left, vec![2.0, 1.0]);
    }

    #[test]
    fn mismatched_lengths_have_no_contingency() {
        assert!(contingency(&[0, 1], &[0]).is_none());
    }

    #[test]
    fn the_pair_count_is_n_choose_two() {
        assert_eq!(pairs(4.0), 6.0);
        assert_eq!(pairs(1.0), 0.0);
        assert_eq!(pairs(0.0), 0.0);
    }
}
