//! CSR adjacency structure — port of `neighbors.py::_build_adj_csr`.

use crate::Pair;

/// An undirected graph in compressed sparse row form.
///
/// Both directions of each edge are stored, so `neighbours(i)` is a contiguous
/// slice. This is the layout the Python fast path builds before handing it to
/// its Numba BFS kernel; keeping the same shape means the traversal has the
/// same memory behaviour without needing a JIT.
#[derive(Debug, Clone)]
pub struct Adjacency {
    /// Neighbour ids, grouped by source node.
    indices: Vec<u32>,
    /// `indptr[i]..indptr[i + 1]` delimits the neighbours of node `i`.
    indptr: Vec<u32>,
}

impl Adjacency {
    /// Build the adjacency of a network with `n_nodes` nodes.
    ///
    /// Edges whose endpoints fall outside `0..n_nodes` are skipped rather than
    /// panicking: a hand-edited edges file is a realistic input, and dropping
    /// the bad row keeps the rest of the sample usable.
    pub fn from_pairs(pairs: &[Pair], n_nodes: usize) -> Self {
        let mut degree = vec![0u32; n_nodes];
        for &(a, b) in pairs {
            if (a as usize) < n_nodes && (b as usize) < n_nodes {
                degree[a as usize] += 1;
                degree[b as usize] += 1;
            }
        }

        let mut indptr = Vec::with_capacity(n_nodes + 1);
        indptr.push(0u32);
        let mut running = 0u32;
        for &d in &degree {
            running += d;
            indptr.push(running);
        }

        let mut indices = vec![0u32; running as usize];
        let mut cursor: Vec<u32> = indptr[..n_nodes].to_vec();
        for &(a, b) in pairs {
            if (a as usize) < n_nodes && (b as usize) < n_nodes {
                indices[cursor[a as usize] as usize] = b;
                cursor[a as usize] += 1;
                indices[cursor[b as usize] as usize] = a;
                cursor[b as usize] += 1;
            }
        }

        Self { indices, indptr }
    }

    /// Number of nodes.
    pub fn n_nodes(&self) -> usize {
        self.indptr.len().saturating_sub(1)
    }

    /// Neighbours of node `i`.
    pub fn neighbours(&self, i: usize) -> &[u32] {
        let start = self.indptr[i] as usize;
        let end = self.indptr[i + 1] as usize;
        &self.indices[start..end]
    }

    /// Degree of node `i`.
    pub fn degree(&self, i: usize) -> usize {
        (self.indptr[i + 1] - self.indptr[i]) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_both_directions_of_each_edge() {
        let adj = Adjacency::from_pairs(&[(0, 1), (1, 2)], 3);
        assert_eq!(adj.n_nodes(), 3);
        assert_eq!(adj.neighbours(0), &[1]);

        let mut mid = adj.neighbours(1).to_vec();
        mid.sort_unstable();
        assert_eq!(mid, vec![0, 2]);
        assert_eq!(adj.neighbours(2), &[1]);
    }

    #[test]
    fn isolated_nodes_have_an_empty_slice() {
        let adj = Adjacency::from_pairs(&[(0, 1)], 4);
        assert!(adj.neighbours(3).is_empty());
        assert_eq!(adj.degree(3), 0);
    }

    #[test]
    fn out_of_range_edges_are_skipped() {
        let adj = Adjacency::from_pairs(&[(0, 1), (0, 99)], 2);
        assert_eq!(adj.neighbours(0), &[1]);
        assert_eq!(adj.degree(0), 1);
    }

    #[test]
    fn an_empty_network_is_valid() {
        let adj = Adjacency::from_pairs(&[], 0);
        assert_eq!(adj.n_nodes(), 0);
    }
}
