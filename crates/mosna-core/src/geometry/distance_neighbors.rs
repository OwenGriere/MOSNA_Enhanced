//! Port of `tysserand::distance_neighbors`.

use crate::{Pair, Point2};

/// Euclidean length of every edge.
///
/// ```python
/// c0 = coords[pairs[:, 0]]
/// c1 = coords[pairs[:, 1]]
/// distances = np.sqrt(((c0 - c1) ** 2).sum(axis=1))
/// ```
pub fn distance_neighbors(coords: &[Point2], pairs: &[Pair]) -> Vec<f64> {
    pairs
        .iter()
        .map(|&(a, b)| {
            let p = coords[a as usize];
            let q = coords[b as usize];
            let dx = p[0] - q[0];
            let dy = p[1] - q[1];
            (dx * dx + dy * dy).sqrt()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_euclidean_lengths() {
        let coords = vec![[0.0, 0.0], [3.0, 4.0], [3.0, 0.0]];
        let pairs = vec![(0, 1), (0, 2)];
        assert_eq!(distance_neighbors(&coords, &pairs), vec![5.0, 3.0]);
    }

    #[test]
    fn a_self_edge_has_length_zero() {
        let coords = vec![[1.0, 1.0]];
        assert_eq!(distance_neighbors(&coords, &[(0, 0)]), vec![0.0]);
    }
}
