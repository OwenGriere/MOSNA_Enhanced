//! Port of `tysserand::find_trim_dist`.

use crate::stats::percentile::percentile;

/// The edge-length threshold above which reconstructed edges are discarded.
///
/// ```python
/// if method == 'percentile_size':
///     prop_edges = 4 / nb_nodes**0.5
///     perc = 100 * (1 - prop_edges * 0.5)
///     dist_thresh = np.percentile(dist, perc)
/// elif method == 'percentile':
///     dist_thresh = np.percentile(dist, perc)
/// ```
///
/// The `percentile_size` method adapts the cutoff to the network size: a larger
/// network keeps a larger fraction of its edges, because its Delaunay hull
/// contributes proportionally fewer of the long spurious edges the trimming is
/// there to remove.
///
/// Returns `None` when `dist` is empty, where numpy would raise.
pub fn find_trim_dist(dist: &[f64], method: TrimMethod, nb_nodes: usize, perc: f64) -> Option<f64> {
    match method {
        TrimMethod::PercentileSize => {
            // `nb_nodes ** 0.5` is a float division in Python; a zero-node
            // network never reaches here because trimming needs edges.
            let prop_edges = 4.0 / (nb_nodes as f64).sqrt();
            let adaptive_perc = 100.0 * (1.0 - prop_edges * 0.5);
            percentile(dist, adaptive_perc)
        }
        TrimMethod::Percentile => percentile(dist, perc),
    }
}

/// The two threshold methods `find_trim_dist` implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimMethod {
    /// Percentile adapted to the number of nodes. The tysserand default.
    PercentileSize,
    /// A fixed percentile.
    Percentile,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_size_adapts_to_the_node_count() {
        let dist: Vec<f64> = (1..=100).map(|i| i as f64).collect();

        // 400 nodes -> prop_edges = 0.2 -> perc = 90
        let large = find_trim_dist(&dist, TrimMethod::PercentileSize, 400, 99.0).unwrap();
        // 100 nodes -> prop_edges = 0.4 -> perc = 80
        let small = find_trim_dist(&dist, TrimMethod::PercentileSize, 100, 99.0).unwrap();

        assert!(
            large > small,
            "a larger network must keep longer edges: {large} vs {small}"
        );
        // np.percentile(1..100, 90) == 90.1
        assert!((large - 90.1).abs() < 1e-9, "got {large}");
        assert!((small - 80.2).abs() < 1e-9, "got {small}");
    }

    #[test]
    fn fixed_percentile_ignores_the_node_count() {
        let dist: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let a = find_trim_dist(&dist, TrimMethod::Percentile, 10, 50.0).unwrap();
        let b = find_trim_dist(&dist, TrimMethod::Percentile, 10_000, 50.0).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, 50.5);
    }

    #[test]
    fn an_empty_distance_list_has_no_threshold() {
        assert_eq!(
            find_trim_dist(&[], TrimMethod::PercentileSize, 10, 99.0),
            None
        );
    }
}
