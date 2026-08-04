//! K-order neighbourhood traversal — port of `neighbors.py::_bfs_csr`.

use crate::nas::adjacency::Adjacency;

/// Scratch buffers reused across nodes.
///
/// The Python kernel allocates three `nb_nodes`-sized arrays per node inside a
/// `prange` loop. Reusing one buffer per worker instead keeps the traversal in
/// cache and removes the allocation from the hot path, which is where most of
/// the wall time of a niche run is spent.
pub struct BfsScratch {
    /// Generation marker per node, so the visited set is cleared in O(1).
    stamp: Vec<u32>,
    generation: u32,
    result: Vec<u32>,
    frontier: Vec<u32>,
    next_frontier: Vec<u32>,
}

impl BfsScratch {
    /// Allocate buffers for a network of `n_nodes` nodes.
    pub fn new(n_nodes: usize) -> Self {
        Self {
            stamp: vec![0; n_nodes],
            generation: 0,
            result: Vec::with_capacity(n_nodes),
            frontier: Vec::with_capacity(n_nodes),
            next_frontier: Vec::with_capacity(n_nodes),
        }
    }

    /// Every node reachable from `start` within `order` hops, `start` included.
    ///
    /// Equivalent to `flatten_neighbors(neighbors_k_order(pairs, n, order))`:
    /// the node itself first, then each successive ring in discovery order.
    /// Ordering does not affect the mean and standard deviation computed over
    /// the set, but it is kept because it makes the traversal directly
    /// comparable with the Python one when debugging.
    pub fn neighbourhood(&mut self, adj: &Adjacency, start: usize, order: usize) -> &[u32] {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // Wrapped around: clear so stale stamps cannot alias.
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.generation = 1;
        }
        let generation = self.generation;

        self.result.clear();
        self.frontier.clear();

        self.stamp[start] = generation;
        self.result.push(start as u32);
        self.frontier.push(start as u32);

        for _ in 0..order {
            self.next_frontier.clear();
            for &node in &self.frontier {
                for &neighbour in adj.neighbours(node as usize) {
                    if self.stamp[neighbour as usize] != generation {
                        self.stamp[neighbour as usize] = generation;
                        self.next_frontier.push(neighbour);
                        self.result.push(neighbour);
                    }
                }
            }
            if self.next_frontier.is_empty() {
                break;
            }
            std::mem::swap(&mut self.frontier, &mut self.next_frontier);
        }

        &self.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path graph 0 - 1 - 2 - 3 - 4.
    fn path() -> Adjacency {
        Adjacency::from_pairs(&[(0, 1), (1, 2), (2, 3), (3, 4)], 5)
    }

    #[test]
    fn order_one_returns_the_node_and_its_direct_neighbours() {
        let adj = path();
        let mut scratch = BfsScratch::new(5);
        let mut got = scratch.neighbourhood(&adj, 2, 1).to_vec();
        got.sort_unstable();
        assert_eq!(got, vec![1, 2, 3]);
    }

    #[test]
    fn order_two_reaches_two_hops_away() {
        let adj = path();
        let mut scratch = BfsScratch::new(5);
        let mut got = scratch.neighbourhood(&adj, 2, 2).to_vec();
        got.sort_unstable();
        assert_eq!(got, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_start_node_comes_first() {
        let adj = path();
        let mut scratch = BfsScratch::new(5);
        assert_eq!(scratch.neighbourhood(&adj, 3, 2)[0], 3);
    }

    #[test]
    fn an_isolated_node_is_its_own_neighbourhood() {
        let adj = Adjacency::from_pairs(&[(0, 1)], 3);
        let mut scratch = BfsScratch::new(3);
        assert_eq!(scratch.neighbourhood(&adj, 2, 3), &[2]);
    }

    #[test]
    fn each_node_is_reported_once() {
        // A triangle, where a naive traversal would revisit nodes.
        let adj = Adjacency::from_pairs(&[(0, 1), (1, 2), (0, 2)], 3);
        let mut scratch = BfsScratch::new(3);
        let got = scratch.neighbourhood(&adj, 0, 3).to_vec();
        let mut unique = got.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(got.len(), unique.len());
        assert_eq!(unique, vec![0, 1, 2]);
    }

    #[test]
    fn scratch_is_reusable_across_nodes() {
        let adj = path();
        let mut scratch = BfsScratch::new(5);
        for _ in 0..3 {
            let a = scratch.neighbourhood(&adj, 0, 1).to_vec();
            assert_eq!(a, vec![0, 1]);
            let b = scratch.neighbourhood(&adj, 4, 1).to_vec();
            assert_eq!(b, vec![4, 3]);
        }
    }
}
