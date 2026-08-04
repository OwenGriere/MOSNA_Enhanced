//! Port of `assortativity.py::mixing_matrix`.

use crate::Pair;

/// A dense symmetric `n x n` matrix, stored row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct MixMat {
    pub n: usize,
    pub values: Vec<f64>,
}

impl MixMat {
    pub fn zeros(n: usize) -> Self {
        Self {
            n,
            values: vec![0.0; n * n],
        }
    }

    #[inline]
    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.values[i * self.n + j]
    }

    #[inline]
    pub fn set(&mut self, i: usize, j: usize, value: f64) {
        self.values[i * self.n + j] = value;
    }

    /// Sum of every element.
    pub fn sum(&self) -> f64 {
        self.values.iter().sum()
    }

    /// Sum of the diagonal.
    pub fn trace(&self) -> f64 {
        (0..self.n).map(|i| self.get(i, i)).sum()
    }
}

/// Mixing matrix of a network whose nodes carry one-hot attributes.
///
/// Port of
///
/// ```python
/// src = nodes.loc[edges['source'], attributes].values.astype(float)
/// tgt = nodes.loc[edges['target'], attributes].values.astype(float)
/// mixmat = src.T @ tgt + tgt.T @ src
/// if not double_diag:
///     np.fill_diagonal(mixmat, np.diag(mixmat) / 2)
/// if normalized:
///     mixmat = mixmat / mixmat.sum()
/// ```
///
/// The `src.T @ tgt + tgt.T @ src` form makes the matrix symmetric and doubles
/// the diagonal, matching how NetworkX and iGraph report an undirected mixing
/// matrix.
///
/// `assignments[node]` is the attribute index of that node, or `None` when the
/// node carries no attribute in the current vocabulary. Taking the assignment
/// rather than the full one-hot matrix turns the two dense matrix products into
/// a single pass over the edges, which is what makes the shuffling loop —
/// hundreds of repetitions per sample — affordable.
pub fn mixing_matrix(
    assignments: &[Option<u32>],
    pairs: &[Pair],
    n_attributes: usize,
    normalized: bool,
    double_diag: bool,
) -> MixMat {
    let mut mixmat = MixMat::zeros(n_attributes);

    for &(source, target) in pairs {
        let (Some(a), Some(b)) = (
            assignments.get(source as usize).copied().flatten(),
            assignments.get(target as usize).copied().flatten(),
        ) else {
            continue;
        };
        let (a, b) = (a as usize, b as usize);
        // `src.T @ tgt` contributes (a, b); `tgt.T @ src` contributes (b, a).
        // For a == b both land on the diagonal, doubling it.
        mixmat.values[a * n_attributes + b] += 1.0;
        mixmat.values[b * n_attributes + a] += 1.0;
    }

    if !double_diag {
        for i in 0..n_attributes {
            let halved = mixmat.get(i, i) / 2.0;
            mixmat.set(i, i, halved);
        }
    }

    if normalized {
        let total = mixmat.sum();
        if total != 0.0 {
            for value in &mut mixmat.values {
                *value /= total;
            }
        }
    }

    mixmat
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_edge_in_both_directions() {
        // Two nodes of attributes 0 and 1, one edge between them.
        let assignments = vec![Some(0), Some(1)];
        let m = mixing_matrix(&assignments, &[(0, 1)], 2, false, true);
        assert_eq!(m.get(0, 1), 1.0);
        assert_eq!(m.get(1, 0), 1.0);
        assert_eq!(m.get(0, 0), 0.0);
        assert_eq!(m.sum(), 2.0);
    }

    #[test]
    fn the_diagonal_is_doubled_like_networkx() {
        // Both endpoints share attribute 0.
        let assignments = vec![Some(0), Some(0)];
        let m = mixing_matrix(&assignments, &[(0, 1)], 1, false, true);
        assert_eq!(m.get(0, 0), 2.0);

        let halved = mixing_matrix(&assignments, &[(0, 1)], 1, false, false);
        assert_eq!(halved.get(0, 0), 1.0);
    }

    #[test]
    fn normalisation_makes_the_matrix_sum_to_one() {
        let assignments = vec![Some(0), Some(1), Some(1)];
        let m = mixing_matrix(&assignments, &[(0, 1), (1, 2)], 2, true, true);
        assert!((m.sum() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_matrix_is_symmetric() {
        let assignments = vec![Some(0), Some(1), Some(2), Some(0)];
        let m = mixing_matrix(
            &assignments,
            &[(0, 1), (1, 2), (2, 3), (0, 2)],
            3,
            true,
            true,
        );
        for i in 0..3 {
            for j in 0..3 {
                assert!((m.get(i, j) - m.get(j, i)).abs() < 1e-15);
            }
        }
    }

    #[test]
    fn nodes_without_an_attribute_are_skipped() {
        let assignments = vec![Some(0), None, Some(1)];
        let m = mixing_matrix(&assignments, &[(0, 1), (0, 2)], 2, false, true);
        // Only the 0-2 edge counts.
        assert_eq!(m.sum(), 2.0);
        assert_eq!(m.get(0, 1), 1.0);
    }

    #[test]
    fn an_edgeless_network_yields_zeros() {
        let m = mixing_matrix(&[Some(0)], &[], 1, true, true);
        assert_eq!(m.sum(), 0.0, "normalising zero must not produce NaN");
    }
}
