//! Port of `tysserand::link_solitaries`.

use std::collections::BTreeSet;

use crate::error::Result;
use crate::geometry::build_delaunay::build_delaunay_untrimmed;
use crate::geometry::build_knn::build_knn;
use crate::geometry::distance_neighbors::distance_neighbors;
use crate::geometry::remove_duplicate_pairs::remove_duplicate_pairs;
use crate::{Pair, Point2};

/// How under-connected nodes are reconnected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMethod {
    /// Take the shortest edges available in the untrimmed Delaunay graph.
    Delaunay,
    /// Take the `min_neighbors` nearest neighbours.
    Knn,
}

impl LinkMethod {
    pub fn parse(s: &str) -> Self {
        match s {
            "knn" => LinkMethod::Knn,
            _ => LinkMethod::Delaunay,
        }
    }
}

/// Give every node at least `min_neighbors` edges.
///
/// Trimming long edges leaves isolated or barely-connected nodes behind, and a
/// node with no edge contributes nothing to the neighbourhood statistics — its
/// NAS feature vector would be its own attributes alone. This adds back the
/// shortest edges each such node was denied.
///
/// Faithful to the Python, including the detail that a node is considered
/// under-connected when its *degree counted over the edge list* is below
/// `min_neighbors`, and that a node is only reconnected when at least
/// `min_neighbors` candidate edges exist for it.
pub fn link_solitaries(
    coords: &[Point2],
    pairs: &[Pair],
    method: LinkMethod,
    min_neighbors: usize,
) -> Result<Vec<Pair>> {
    let n_nodes = coords.len();
    if n_nodes == 0 {
        return Ok(pairs.to_vec());
    }

    let solitaries = under_connected(pairs, n_nodes, min_neighbors);
    if solitaries.is_empty() {
        return Ok(pairs.to_vec());
    }

    let mut out = pairs.to_vec();
    match method {
        LinkMethod::Delaunay => {
            let all_pairs = build_delaunay_untrimmed(coords)?;
            let all_dist = distance_neighbors(coords, &all_pairs);

            for node in solitaries {
                // Indices of every candidate edge touching this node.
                let candidates: Vec<usize> = all_pairs
                    .iter()
                    .enumerate()
                    .filter(|(_, &(a, b))| a == node || b == node)
                    .map(|(i, _)| i)
                    .collect();

                // `if len(node_distances) >= min_neighbors` — a node the
                // triangulation cannot serve is left as it is.
                if candidates.len() < min_neighbors {
                    continue;
                }
                let mut ranked = candidates;
                ranked.sort_by(|&a, &b| {
                    all_dist[a]
                        .partial_cmp(&all_dist[b])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for &idx in ranked.iter().take(min_neighbors) {
                    out.push(all_pairs[idx]);
                }
            }
        }
        LinkMethod::Knn => {
            let nn_pairs = build_knn(coords, min_neighbors);
            for node in solitaries {
                for &pair in nn_pairs.iter().filter(|&&(a, b)| a == node || b == node) {
                    out.push(pair);
                }
            }
        }
    }

    Ok(remove_duplicate_pairs(out))
}

/// Nodes with fewer than `min_neighbors` incident edges.
///
/// Mirrors the Python, which unions the nodes absent from the edge list with
/// those whose count in `np.unique(pairs, return_counts=True)` is below the
/// threshold.
fn under_connected(pairs: &[Pair], n_nodes: usize, min_neighbors: usize) -> BTreeSet<u32> {
    let mut degree = vec![0usize; n_nodes];
    for &(a, b) in pairs {
        if (a as usize) < n_nodes {
            degree[a as usize] += 1;
        }
        if (b as usize) < n_nodes {
            degree[b as usize] += 1;
        }
    }
    let threshold = min_neighbors.max(1);
    (0..n_nodes as u32)
        .filter(|&i| degree[i as usize] < threshold)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The docstring example: five points on a line, only the first three
    /// linked, `min_neighbors=1` must connect node 3 and node 4.
    #[test]
    fn reconnects_isolated_nodes_with_knn() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.1, 0.0], [4.0, 0.0]];
        let pairs = vec![(0, 1), (1, 2)];
        let out = link_solitaries(&coords, &pairs, LinkMethod::Knn, 1).unwrap();
        assert_eq!(out, vec![(0, 1), (1, 2), (3, 4)]);
    }

    #[test]
    fn a_higher_min_neighbors_adds_more_edges() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.1, 0.0], [4.0, 0.0]];
        let pairs = vec![(0, 1), (1, 2)];
        let out = link_solitaries(&coords, &pairs, LinkMethod::Knn, 2).unwrap();
        assert!(out.len() > 3, "got {out:?}");
        // Nodes 3 and 4 had no edges at all and must now have some.
        assert!(out.iter().any(|&(a, b)| a == 3 || b == 3));
        assert!(out.iter().any(|&(a, b)| a == 4 || b == 4));
    }

    #[test]
    fn a_fully_connected_network_is_returned_unchanged() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
        let pairs = vec![(0, 1), (1, 2), (0, 2)];
        let out = link_solitaries(&coords, &pairs, LinkMethod::Delaunay, 2).unwrap();
        assert_eq!(out, pairs);
    }

    #[test]
    fn delaunay_relinking_connects_every_node() {
        // A grid, with the edge list emptied so every node is solitary.
        let mut coords = Vec::new();
        for i in 0..5 {
            for j in 0..5 {
                coords.push([i as f64, j as f64]);
            }
        }
        let out = link_solitaries(&coords, &[], LinkMethod::Delaunay, 3).unwrap();

        let mut degree = vec![0usize; coords.len()];
        for &(a, b) in &out {
            degree[a as usize] += 1;
            degree[b as usize] += 1;
        }
        assert!(
            degree.iter().all(|&d| d >= 2),
            "no node may be left isolated, got {degree:?}"
        );
        // Interior nodes, which have plenty of Delaunay candidates, do reach
        // the requested degree.
        assert!(degree[12] >= 3, "the centre node must be well connected");
    }

    /// The Python guards each reconnection with
    /// `if len(node_distances) >= min_neighbors`, so a node the triangulation
    /// simply cannot serve — a hull corner with only two incident Delaunay
    /// edges — is left below the threshold rather than being wired to distant
    /// nodes. That is reproduced here, and this test pins it: raising a corner
    /// node's degree by relaxing the guard would connect cells that are not
    /// spatial neighbours and quietly distort every neighbourhood statistic.
    #[test]
    fn a_node_with_too_few_candidates_is_left_alone() {
        // A triangle plus a far-away point. The far point has few Delaunay
        // edges, so asking for a high min_neighbors cannot be satisfied.
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [50.0, 50.0]];
        let out = link_solitaries(&coords, &[], LinkMethod::Delaunay, 3).unwrap();

        let degree_of_far = out.iter().filter(|&&(a, b)| a == 3 || b == 3).count();
        assert!(
            degree_of_far < 3,
            "the isolated point must not be force-connected, got degree {degree_of_far}"
        );
    }

    #[test]
    fn relinking_never_duplicates_an_existing_edge() {
        let coords = vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [10.0, 10.0]];
        let pairs = vec![(0, 1)];
        let out = link_solitaries(&coords, &pairs, LinkMethod::Delaunay, 2).unwrap();
        let mut unique = out.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(out, unique);
    }

    #[test]
    fn under_connected_counts_degree_not_membership() {
        // Node 1 appears in the edge list but only once.
        let solitaries = under_connected(&[(0, 1)], 3, 2);
        assert_eq!(solitaries.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn an_empty_network_is_handled() {
        assert!(link_solitaries(&[], &[], LinkMethod::Delaunay, 3)
            .unwrap()
            .is_empty());
    }
}
