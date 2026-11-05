//! Port of `tysserand::build_delaunay`.

use crate::error::{CoreError, Result};
use crate::geometry::distance_neighbors::distance_neighbors;
use crate::geometry::find_trim_dist::{find_trim_dist, TrimMethod};
use crate::geometry::remove_duplicate_pairs::remove_duplicate_pairs;
use crate::{Pair, Point2};

/// How long edges are discarded after triangulation, by a single global
/// threshold.
///
/// This is tysserand's `trim_dist` argument, whose default is `False` — no
/// global trimming at all. The trimming that *is* on by default is
/// [`NodeAdaptive`], which is a different rule entirely.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrimDist {
    /// Keep every edge (`trim_dist=False`). The tysserand default.
    None,
    /// Discard edges at or above this length (`trim_dist=<float>`).
    Fixed(f64),
    /// Discard edges at or above a percentile of the length distribution.
    Method(TrimMethod, f64),
}

impl Default for TrimDist {
    /// `trim_dist=False`, which is what `build_delaunay(coords)` gets.
    fn default() -> Self {
        TrimDist::None
    }
}

/// Trimming with a threshold computed per node, from its own neighbourhood.
///
/// This is what tysserand actually applies by default
/// (`node_adaptive_trimming=True`), and what `draw_per_sample` therefore gets.
/// For each node, the threshold is its `n_edges`-th shortest incident edge
/// multiplied by `trim_dist_ratio`; incident edges longer than both that and
/// `min_dist` are discarded.
///
/// The rule is *local*, which is its whole point: in a tissue whose density
/// varies, an edge that is long in a dense region is an artefact while the same
/// length in a sparse region is the real neighbourhood. A single global
/// threshold cannot express that, and the two rules disagree on about ten per
/// cent of the edges — see `test/parity`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeAdaptive {
    /// Which of the node's shortest incident edges sets the scale.
    pub n_edges: usize,
    /// Multiplier applied to it.
    pub trim_dist_ratio: f64,
    /// An edge shorter than this is never discarded.
    pub min_dist: f64,
}

impl Default for NodeAdaptive {
    /// `n_edges=3, trim_dist_ratio=2, min_dist=0` — tysserand's defaults.
    fn default() -> Self {
        Self {
            n_edges: 3,
            trim_dist_ratio: 2.0,
            min_dist: 0.0,
        }
    }
}

/// Everything `build_delaunay` may discard, in the order tysserand applies it:
/// the per-node rule first, then the global one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelaunayTrim {
    /// `None` disables it, as `node_adaptive_trimming=False` does.
    pub node_adaptive: Option<NodeAdaptive>,
    pub distance: TrimDist,
}

impl Default for DelaunayTrim {
    /// What `ty.build_delaunay(coords)` does with no arguments: the per-node
    /// rule on, the global threshold off.
    fn default() -> Self {
        Self {
            node_adaptive: Some(NodeAdaptive::default()),
            distance: TrimDist::None,
        }
    }
}

impl DelaunayTrim {
    /// Keep every edge the triangulation produced.
    pub fn none() -> Self {
        Self {
            node_adaptive: None,
            distance: TrimDist::None,
        }
    }

    /// Only a global threshold, with no per-node rule.
    pub fn distance(distance: TrimDist) -> Self {
        Self {
            node_adaptive: None,
            distance,
        }
    }
}

/// Reconstruct edges by Delaunay triangulation, then trim the long ones.
///
/// The Python original takes the Delaunay edges from
/// `scipy.spatial.Voronoi(coords).ridge_points` — the pairs of input points
/// whose Voronoi cells share a ridge, which is exactly the Delaunay edge set.
/// This computes the triangulation directly, which is the same set — once the
/// arbitrary diagonals between cocircular points are removed, since Qhull does
/// not emit them (see `is_cocircular_diagonal`).
///
/// # Ordering
///
/// Qhull returns ridges in an unspecified order; this returns them sorted and
/// deduplicated. Every consumer builds an adjacency structure or a mixing
/// matrix from the edge list, both order-independent, so results are unchanged
/// — but the `edges_*.parquet` files become byte-reproducible across runs and
/// machines, which the Python output is not.
pub fn build_delaunay(coords: &[Point2], trim: DelaunayTrim) -> Result<Vec<Pair>> {
    let mut pairs = build_delaunay_untrimmed(coords)?;

    if let Some(adaptive) = trim.node_adaptive {
        pairs = trim_node_adaptive(coords, pairs, adaptive);
    }

    let threshold = match trim.distance {
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

/// Discard, for each node in turn, the incident edges that are long compared
/// with that node's own shortest ones.
///
/// Port of the `node_adaptive_trimming` block of `tysserand.build_delaunay`:
///
/// ```python
/// dist = distance_neighbors(coords, pairs)
/// for node_id in range(len(coords)):
///     pairs_ids = indices of edges touching node_id
///     node_distances = dist[pairs_ids]
///     if len(node_distances) > n_edges:
///         thresh_dist = np.sort(node_distances)[n_edges-1] * trim_dist_ratio
///         discard = (node_distances > thresh_dist) & (node_distances > min_dist)
///         pairs = np.delete(pairs, pairs_ids[discard], axis=0)
///         dist  = np.delete(dist,  pairs_ids[discard])
/// ```
///
/// # The loop is sequential, and that matters
///
/// The Python deletes from `pairs` *inside* the loop, so node `n` sees the edge
/// list as nodes `0..n` left it. A node whose long edges were already removed
/// by a neighbour therefore computes a different threshold than it would have
/// on the original graph. That is reproduced here rather than parallelised: the
/// result depends on the node order, and the node order is `0..n`.
///
/// What does *not* matter is the order of the edge array itself — every step is
/// expressed over the set of edges incident to a node, so this yields the same
/// result as the Python despite the pairs being sorted here and in Qhull order
/// there.
fn trim_node_adaptive(coords: &[Point2], pairs: Vec<Pair>, params: NodeAdaptive) -> Vec<Pair> {
    if params.n_edges == 0 || pairs.is_empty() {
        return pairs;
    }

    let dist = distance_neighbors(coords, &pairs);

    // `false` marks an edge already discarded; compacting the vectors at every
    // node would be quadratic in the edge count.
    let mut alive = vec![true; pairs.len()];

    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); coords.len()];
    for (index, &(a, b)) in pairs.iter().enumerate() {
        incident[a as usize].push(index);
        incident[b as usize].push(index);
    }

    let mut lengths: Vec<f64> = Vec::new();
    // Node by node, in index order — see the note above on why this is not a
    // parallel loop.
    for edges_here in &incident {
        lengths.clear();
        lengths.extend(
            edges_here
                .iter()
                .filter(|&&index| alive[index])
                .map(|&index| dist[index]),
        );

        // `if len(node_distances) > n_edges` — a node with no more edges than
        // the rule needs to measure itself is left alone.
        if lengths.len() <= params.n_edges {
            continue;
        }
        lengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let threshold = lengths[params.n_edges - 1] * params.trim_dist_ratio;

        for &index in edges_here {
            if alive[index] && dist[index] > threshold && dist[index] > params.min_dist {
                alive[index] = false;
            }
        }
    }

    pairs
        .into_iter()
        .zip(alive)
        .filter(|(_, alive)| *alive)
        .map(|(pair, _)| pair)
        .collect()
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

    let triangles = &triangulation.triangles;
    let mut pairs = Vec::with_capacity(triangles.len());
    for e in 0..triangles.len() {
        // Emit each undirected edge once: on a boundary half-edge, or on the
        // lower-indexed side of an interior pair.
        let opposite = triangulation.halfedges[e];
        if opposite == delaunator::EMPTY {
            pairs.push((triangles[e] as u32, triangles[next_halfedge(e)] as u32));
        } else if e < opposite {
            if is_cocircular_diagonal(coords, triangles, e, opposite) {
                continue;
            }
            pairs.push((triangles[e] as u32, triangles[next_halfedge(e)] as u32));
        }
    }
    Ok(remove_duplicate_pairs(pairs))
}

/// Whether the interior edge shared by half-edges `e` and `opposite` is an
/// arbitrary diagonal of cocircular points, which Qhull does not emit.
///
/// When four or more points lie on one circle, the Delaunay triangulation is
/// not unique: any diagonal of the polygon they form is valid. `delaunator`
/// picks one. Qhull — behind `scipy.spatial.Voronoi(coords).ridge_points`,
/// which is what tysserand reads — picks none: it merges the cocircular
/// triangles into a single non-simplicial facet, and only the polygon's sides
/// separate two Voronoi cells with a ridge. Four cells on a perfect square
/// therefore get four edges in tysserand, not five.
///
/// This happens all the time with integer coordinates (centroids in pixels),
/// and never in practice with floating-point ones. The edge is dropped exactly
/// when the vertex across it lies *on* the circumcircle of the triangle on this
/// side, decided by an exact predicate: a tolerance would start removing edges
/// tysserand keeps.
fn is_cocircular_diagonal(
    coords: &[Point2],
    triangles: &[usize],
    e: usize,
    opposite: usize,
) -> bool {
    let coord = |vertex: usize| robust::Coord {
        x: coords[vertex][0],
        y: coords[vertex][1],
    };
    // The triangle on this side, in delaunator's winding order, and the vertex
    // of the neighbouring triangle that is not on the shared edge.
    let a = coord(triangles[e]);
    let b = coord(triangles[next_halfedge(e)]);
    let c = coord(triangles[prev_halfedge(e)]);
    let d = coord(triangles[prev_halfedge(opposite)]);
    robust::incircle(a, b, c, d) == 0.0
}

/// Index of the next half-edge within the same triangle.
fn next_halfedge(e: usize) -> usize {
    if e % 3 == 2 {
        e - 2
    } else {
        e + 1
    }
}

/// Index of the previous half-edge within the same triangle.
fn prev_halfedge(e: usize) -> usize {
    if e % 3 == 0 {
        e + 2
    } else {
        e - 1
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

    /// A unit square: its four corners are cocircular, so Qhull emits the four
    /// sides and no diagonal — `Voronoi([[0,0],[1,0],[1,1],[0,1]]).ridge_points`
    /// has four rows.
    #[test]
    fn a_square_has_no_diagonal() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let pairs = build_delaunay(&coords, DelaunayTrim::none()).unwrap();
        assert_eq!(pairs, vec![(0, 1), (0, 3), (1, 2), (2, 3)]);
    }

    /// A slightly squashed square is not cocircular: the triangulation is
    /// unique and its diagonal is a genuine Delaunay edge.
    #[test]
    fn a_non_cocircular_quadrilateral_keeps_its_diagonal() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.1]];
        let pairs = build_delaunay(&coords, DelaunayTrim::none()).unwrap();
        assert_eq!(pairs.len(), 5, "got {pairs:?}");
    }

    /// A 4 × 4 integer grid: Qhull gives the 24 axis-aligned edges and none of
    /// the 9 diagonals a triangulation would add.
    #[test]
    fn an_integer_grid_matches_qhull() {
        let coords: Vec<Point2> = (0..4)
            .flat_map(|i| (0..4).map(move |j| [i as f64, j as f64]))
            .collect();
        let pairs = build_delaunay(&coords, DelaunayTrim::none()).unwrap();
        assert_eq!(pairs.len(), 24, "got {pairs:?}");
        let dist = distance_neighbors(&coords, &pairs);
        assert!(dist.iter().all(|&d| d == 1.0));
    }

    /// Six points on one circle form a single Qhull facet: only the hexagon's
    /// sides remain, whatever diagonals `delaunator` chose inside it.
    #[test]
    fn cocircular_polygon_keeps_only_its_sides() {
        // Exactly cocircular in floating point: the 3-4-5 points of radius 5.
        let coords = vec![
            [5.0, 0.0],
            [3.0, 4.0],
            [-3.0, 4.0],
            [-5.0, 0.0],
            [-3.0, -4.0],
            [3.0, -4.0],
        ];
        let pairs = build_delaunay(&coords, DelaunayTrim::none()).unwrap();
        assert_eq!(pairs, vec![(0, 1), (0, 5), (1, 2), (2, 3), (3, 4), (4, 5)]);
    }

    #[test]
    fn output_is_sorted_and_deduplicated() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [0.5, 0.4]];
        let pairs = build_delaunay(&coords, DelaunayTrim::none()).unwrap();
        let mut expected = pairs.clone();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(pairs, expected);
        assert!(pairs.iter().all(|&(a, b)| a < b));
    }

    /// The per-node rule measures an edge against its own node's
    /// neighbourhood, not against the cohort — which is the whole reason it
    /// exists, and the thing a single global threshold cannot do.
    ///
    /// Two clusters of very different density: the rule must keep edges in the
    /// loose one that are *longer* than the edges it cuts in the tight one. No
    /// global threshold can satisfy both at once.
    ///
    /// The coordinates are jittered on purpose. On a perfect lattice every four
    /// points of a square are cocircular and their diagonal is dropped (see
    /// `a_square_has_no_diagonal`), so a lattice would test that rule rather
    /// than the trimming.
    #[test]
    fn the_node_rule_adapts_its_threshold_to_the_local_density() {
        let jitter = |i: usize, j: usize| ((i * 7 + j * 13) % 5) as f64 * 0.011;
        let mut coords = Vec::new();
        // Tight: five by five, a tenth of a unit apart.
        for i in 0..5 {
            for j in 0..5 {
                coords.push([i as f64 * 0.1 + jitter(i, j) * 0.1, j as f64 * 0.1]);
            }
        }
        let tight = coords.len();
        // Loose: five by five, a whole unit apart, far away.
        for i in 0..5 {
            for j in 0..5 {
                coords.push([80.0 + i as f64 + jitter(i, j), j as f64]);
            }
        }

        let kept = build_delaunay(&coords, DelaunayTrim::default()).unwrap();
        let all = build_delaunay(&coords, DelaunayTrim::none()).unwrap();

        let lengths = |pairs: &[Pair], inside: fn(u32, usize) -> bool, tight: usize| {
            let dist = distance_neighbors(&coords, pairs);
            pairs
                .iter()
                .zip(dist)
                .filter(|((a, b), _)| inside(*a, tight) && inside(*b, tight))
                .map(|(_, d)| d)
                .fold(0.0f64, f64::max)
        };
        let in_tight = |n: u32, tight: usize| (n as usize) < tight;
        let in_loose = |n: u32, tight: usize| (n as usize) >= tight;

        let longest_kept_tight = lengths(&kept, in_tight, tight);
        let longest_kept_loose = lengths(&kept, in_loose, tight);
        let longest_all_tight = lengths(&all, in_tight, tight);

        assert!(
            longest_kept_loose > longest_kept_tight,
            "the loose cluster should keep longer edges than the tight one: \
             {longest_kept_loose} against {longest_kept_tight}"
        );
        // And an edge of that length would have been cut in the tight cluster:
        // no single threshold could have produced this pair of outcomes.
        assert!(
            longest_kept_loose > longest_all_tight,
            "the two regimes do not actually overlap, so the fixture proves nothing"
        );
        // The bridge between the two clusters is gone from both ends.
        let bridges = kept
            .iter()
            .filter(|&&(a, b)| in_tight(a, tight) != in_tight(b, tight))
            .count();
        assert_eq!(bridges, 0, "the crossing edges survived the per-node rule");
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

        let untrimmed = build_delaunay(&coords, DelaunayTrim::none()).unwrap();
        let trimmed =
            build_delaunay(&coords, DelaunayTrim::distance(TrimDist::Fixed(1.0))).unwrap();
        assert!(trimmed.len() < untrimmed.len());
        // No surviving edge may span the gap.
        let dist = distance_neighbors(&coords, &trimmed);
        assert!(dist.iter().all(|&d| d < 1.0));
    }

    #[test]
    fn a_pair_of_points_yields_one_edge() {
        let pairs = build_delaunay(&[[0.0, 0.0], [1.0, 1.0]], DelaunayTrim::none()).unwrap();
        assert_eq!(pairs, vec![(0, 1)]);
    }

    #[test]
    fn a_single_point_yields_no_edge() {
        assert!(build_delaunay(&[[0.0, 0.0]], DelaunayTrim::none())
            .unwrap()
            .is_empty());
        assert!(build_delaunay(&[], DelaunayTrim::none())
            .unwrap()
            .is_empty());
    }

    /// Collinear points make Qhull raise on the Python side; here they produce
    /// the degenerate triangulation, a path.
    #[test]
    fn collinear_points_form_a_path() {
        let coords = vec![[0.0, 0.0], [2.0, 0.0], [1.0, 0.0], [3.0, 0.0]];
        let pairs = build_delaunay(&coords, DelaunayTrim::none()).unwrap();
        assert_eq!(pairs, vec![(0, 2), (1, 2), (1, 3)]);
    }

    #[test]
    fn non_finite_coordinates_are_rejected() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [f64::NAN, 1.0]];
        let err = build_delaunay(&coords, DelaunayTrim::none()).unwrap_err();
        assert!(err.to_string().contains("not finite"));
    }

    /// `ty.build_delaunay(coords)` with no arguments trims per node and applies
    /// no global threshold. This test used to assert the opposite — a global
    /// 99th-percentile trim — which is where the wrong network came from.
    #[test]
    fn the_default_trim_is_the_tysserand_default() {
        let default = DelaunayTrim::default();
        assert_eq!(
            default.node_adaptive,
            Some(NodeAdaptive {
                n_edges: 3,
                trim_dist_ratio: 2.0,
                min_dist: 0.0
            }),
            "`node_adaptive_trimming=True` is the tysserand default"
        );
        assert_eq!(
            default.distance,
            TrimDist::None,
            "`trim_dist=False` is the tysserand default"
        );
    }
}
