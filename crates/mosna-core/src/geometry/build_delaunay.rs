//! Port of `tysserand::build_delaunay`.

use crate::error::{CoreError, Result};
use crate::geometry::distance_neighbors::distance_neighbors;
use crate::geometry::find_trim_dist::{find_trim_dist, TrimMethod};
use crate::geometry::remove_duplicate_pairs::remove_duplicate_pairs;
use crate::{Pair, Point2};

/// How long edges are discarded after triangulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrimDist {
    /// Keep every edge (`trim_dist=False`).
    None,
    /// Discard edges at or above this length (`trim_dist=<float>`).
    Fixed(f64),
    /// Discard edges at or above a percentile of the length distribution.
    Method(TrimMethod, f64),
}

impl Default for TrimDist {
    /// `trim_dist='percentile_size', perc=99` — the tysserand default, and what
    /// `draw_per_sample` gets by calling `build_delaunay(coords)`.
    fn default() -> Self {
        TrimDist::Method(TrimMethod::PercentileSize, 99.0)
    }
}

/// Reconstruct edges by Delaunay triangulation, then trim the long ones.
///
/// The Python original takes the Delaunay edges from
/// `scipy.spatial.Voronoi(coords).ridge_points` — the pairs of input points
/// whose Voronoi cells share a ridge, which is exactly the Delaunay edge set.
/// This computes the triangulation directly, which is the same set.
///
/// # Ordering
///
/// Qhull returns ridges in an unspecified order; this returns them sorted and
/// deduplicated. Every consumer builds an adjacency structure or a mixing
/// matrix from the edge list, both order-independent, so results are unchanged
/// — but the `edges_*.parquet` files become byte-reproducible across runs and
/// machines, which the Python output is not.
pub fn build_delaunay(coords: &[Point2], trim_dist: TrimDist) -> Result<Vec<Pair>> {
    let pairs = build_delaunay_untrimmed(coords)?;

    let threshold = match trim_dist {
        TrimDist::None => return Ok(pairs),
        TrimDist::Fixed(d) => Some(d),
        TrimDist::Method(method, perc) => {
            let dist = distance_neighbors(coords, &pairs);
            find_trim_dist(&dist, method, coords.len(), perc)
        }
    };

    let Some(threshold) = threshold else {
        return Ok(pairs);
    };

    let dist = distance_neighbors(coords, &pairs);
    // `pairs[dist < trim_dist, :]` — strictly less, so an edge exactly at the
    // threshold is dropped.
    Ok(pairs
        .into_iter()
        .zip(dist)
        .filter(|(_, d)| *d < threshold)
        .map(|(pair, _)| pair)
        .collect())
}

/// The raw Delaunay edge set, with no trimming.
///
/// This is what `link_solitaries` asks for when it needs the full set of
/// candidate edges to reconnect an under-connected node.
pub fn build_delaunay_untrimmed(coords: &[Point2]) -> Result<Vec<Pair>> {
    let n = coords.len();
    if n < 2 {
        // A single point has no edges; Python's Voronoi raises here instead.
        return Ok(Vec::new());
    }
    if let Some(bad) = coords
        .iter()
        .position(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return Err(CoreError::Geometry {
            n_points: n,
            reason: format!("coordinates of point {bad} are not finite"),
        });
    }
    if n == 2 {
        return Ok(vec![(0, 1)]);
    }

    let points: Vec<delaunator::Point> = coords
        .iter()
        .map(|p| delaunator::Point { x: p[0], y: p[1] })
        .collect();
    let triangulation = delaunator::triangulate(&points);

    if triangulation.triangles.is_empty() {
        // Every point is collinear (or they are all identical), so there is no
        // triangle. The degenerate Delaunay graph of collinear points is the
        // path connecting them in order along the line, which is what
        // `link_solitaries` would otherwise have to rebuild one node at a time.
        // Python's Voronoi raises a QhullError on this input.
        return Ok(collinear_path(coords));
    }

    let mut pairs = Vec::with_capacity(triangulation.triangles.len());
    for e in 0..triangulation.triangles.len() {
        // Emit each undirected edge once: on a boundary half-edge, or on the
        // lower-indexed side of an interior pair.
        let opposite = triangulation.halfedges[e];
        if opposite == delaunator::EMPTY || e < opposite {
            let a = triangulation.triangles[e] as u32;
            let b = triangulation.triangles[next_halfedge(e)] as u32;
            pairs.push((a, b));
        }
    }
    Ok(remove_duplicate_pairs(pairs))
}

/// Index of the next half-edge within the same triangle.
fn next_halfedge(e: usize) -> usize {
    if e % 3 == 2 {
        e - 2
    } else {
        e + 1
    }
}

/// Connect collinear points in order along their principal direction.
fn collinear_path(coords: &[Point2]) -> Vec<Pair> {
    let first = coords[0];
    // Project onto whichever axis actually varies; for identical points both
    // spans are zero and the resulting order is the input order, which still
    // yields a connected path.
    let span_x = coords
        .iter()
        .fold(0.0f64, |acc, p| acc.max((p[0] - first[0]).abs()));
    let span_y = coords
        .iter()
        .fold(0.0f64, |acc, p| acc.max((p[1] - first[1]).abs()));
    let axis = if span_x >= span_y { 0 } else { 1 };

    let mut order: Vec<u32> = (0..coords.len() as u32).collect();
    order.sort_by(|&a, &b| {
        coords[a as usize][axis]
            .partial_cmp(&coords[b as usize][axis])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    remove_duplicate_pairs(order.windows(2).map(|w| (w[0], w[1])))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit square: the Delaunay triangulation is the four sides plus one
    /// diagonal, so five edges.
    #[test]
    fn triangulates_a_square() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let pairs = build_delaunay(&coords, TrimDist::None).unwrap();
        assert_eq!(pairs.len(), 5, "got {pairs:?}");

        // Every node must be reachable.
        let mut degree = [0usize; 4];
        for &(a, b) in &pairs {
            degree[a as usize] += 1;
            degree[b as usize] += 1;
        }
        assert!(degree.iter().all(|&d| d >= 2));
    }

    #[test]
    fn output_is_sorted_and_deduplicated() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [0.5, 0.4]];
        let pairs = build_delaunay(&coords, TrimDist::None).unwrap();
        let mut expected = pairs.clone();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(pairs, expected);
        assert!(pairs.iter().all(|&(a, b)| a < b));
    }

    #[test]
    fn trimming_removes_the_long_edges() {
        // Two tight clusters far apart: trimming must cut the bridging edges.
        let mut coords = Vec::new();
        for i in 0..6 {
            let t = i as f64 * 0.1;
            coords.push([t, t.sin() * 0.1]);
        }
        for i in 0..6 {
            let t = i as f64 * 0.1;
            coords.push([100.0 + t, t.cos() * 0.1]);
        }

        let untrimmed = build_delaunay(&coords, TrimDist::None).unwrap();
        let trimmed = build_delaunay(&coords, TrimDist::Fixed(1.0)).unwrap();
        assert!(trimmed.len() < untrimmed.len());
        // No surviving edge may span the gap.
        let dist = distance_neighbors(&coords, &trimmed);
        assert!(dist.iter().all(|&d| d < 1.0));
    }

    #[test]
    fn a_pair_of_points_yields_one_edge() {
        let pairs = build_delaunay(&[[0.0, 0.0], [1.0, 1.0]], TrimDist::None).unwrap();
        assert_eq!(pairs, vec![(0, 1)]);
    }

    #[test]
    fn a_single_point_yields_no_edge() {
        assert!(build_delaunay(&[[0.0, 0.0]], TrimDist::None)
            .unwrap()
            .is_empty());
        assert!(build_delaunay(&[], TrimDist::None).unwrap().is_empty());
    }

    /// Collinear points make Qhull raise on the Python side; here they produce
    /// the degenerate triangulation, a path.
    #[test]
    fn collinear_points_form_a_path() {
        let coords = vec![[0.0, 0.0], [2.0, 0.0], [1.0, 0.0], [3.0, 0.0]];
        let pairs = build_delaunay(&coords, TrimDist::None).unwrap();
        assert_eq!(pairs, vec![(0, 2), (1, 2), (1, 3)]);
    }

    #[test]
    fn non_finite_coordinates_are_rejected() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [f64::NAN, 1.0]];
        let err = build_delaunay(&coords, TrimDist::None).unwrap_err();
        assert!(err.to_string().contains("not finite"));
    }

    #[test]
    fn the_default_trim_is_the_tysserand_default() {
        assert_eq!(
            TrimDist::default(),
            TrimDist::Method(TrimMethod::PercentileSize, 99.0)
        );
    }
}
