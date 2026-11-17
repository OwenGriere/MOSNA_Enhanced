//! An undirected network in compressed sparse row form.
//!
//! Every network MOSNA computes is undirected, so each edge `{a, b}` is stored
//! in both rows: `b` among the neighbours of `a`, and `a` among those of `b`.
//! The neighbours of a node are then one contiguous, sorted slice, which is
//! what the neighbourhood aggregation walks, and the length of that slice is
//! the node's degree — the number written beside it in the nodes file.
//!
//! A self-loop `{a, a}` is stored once, in row `a`. Delaunay and k-NN never
//! produce one; an edges file from elsewhere may, and it survives the round
//! trip unchanged rather than being counted twice.
//!
//! Repeated edges are kept, as many times as they were given: an edge list
//! read from a file is reproduced as it was, not silently deduplicated.

/// An undirected edge, as two node indices.
pub type Pair = (u32, u32);

/// Why a set of rows does not describe an undirected network.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CsrError {
    #[error("the edge {a}–{b} names a node past the last one: the network has {n_nodes} node(s)")]
    EdgeOutOfRange { a: u32, b: u32, n_nodes: usize },

    #[error(
        "the degrees add up to {degree_sum} but there are {entries} neighbour entries: \
         the nodes and edges files do not describe the same network"
    )]
    LengthMismatch { degree_sum: u64, entries: usize },

    #[error("node {node} lists {neighbour} as a neighbour, past the last node: the network has {n_nodes} node(s)")]
    NeighbourOutOfRange {
        node: usize,
        neighbour: u32,
        n_nodes: usize,
    },

    #[error(
        "node {a} lists {b} as a neighbour {forward} time(s), but {b} lists {a} \
         {backward} time(s): an undirected network lists every edge from both ends"
    )]
    Asymmetric {
        a: u32,
        b: u32,
        forward: usize,
        backward: usize,
    },

    #[error("{entries} neighbour entries do not fit the 32-bit offsets of the format")]
    TooLarge { entries: u64 },
}

/// An undirected network: `indptr[i]..indptr[i + 1]` delimits the neighbours
/// of node `i` in `indices`, sorted in increasing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Csr {
    indptr: Vec<u32>,
    indices: Vec<u32>,
}

impl Csr {
    /// `n_nodes` nodes and no edge.
    pub fn empty(n_nodes: usize) -> Self {
        Self {
            indptr: vec![0; n_nodes + 1],
            indices: Vec::new(),
        }
    }

    /// Build the network of an edge list over `n_nodes` nodes.
    ///
    /// The direction of each pair does not matter, nor their order. An
    /// endpoint outside `0..n_nodes` is refused, naming the edge.
    pub fn from_pairs(pairs: &[Pair], n_nodes: usize) -> Result<Self, CsrError> {
        let mut degrees = vec![0u64; n_nodes];
        for &(a, b) in pairs {
            if a as usize >= n_nodes || b as usize >= n_nodes {
                return Err(CsrError::EdgeOutOfRange { a, b, n_nodes });
            }
            degrees[a as usize] += 1;
            if a != b {
                degrees[b as usize] += 1;
            }
        }
        let indptr = offsets(degrees.iter().copied())?;

        // Each row filled in place, then sorted: a counting sort by row, so
        // the whole build is linear in the edges plus the sort of each row.
        let mut indices = vec![0u32; indptr[n_nodes] as usize];
        let mut cursor: Vec<u32> = indptr[..n_nodes].to_vec();
        for &(a, b) in pairs {
            indices[cursor[a as usize] as usize] = b;
            cursor[a as usize] += 1;
            if a != b {
                indices[cursor[b as usize] as usize] = a;
                cursor[b as usize] += 1;
            }
        }
        sort_rows(&indptr, &mut indices);
        Ok(Self { indptr, indices })
    }

    /// Rebuild a network from what the two files hold: each node's degree, in
    /// node order, and the neighbours of every node one after the other.
    ///
    /// Everything that makes the pair a network is checked — the degrees cover
    /// the entries exactly, every neighbour is a node, every edge is listed
    /// from both ends — because a nodes file and an edges file from two
    /// different runs would otherwise be read as a network that never existed.
    /// Rows written unsorted are sorted.
    pub fn from_degrees(degrees: &[u32], mut indices: Vec<u32>) -> Result<Self, CsrError> {
        let degree_sum: u64 = degrees.iter().map(|&d| d as u64).sum();
        if degree_sum != indices.len() as u64 {
            return Err(CsrError::LengthMismatch {
                degree_sum,
                entries: indices.len(),
            });
        }
        let indptr = offsets(degrees.iter().map(|&d| d as u64))?;
        let n_nodes = degrees.len();

        for node in 0..n_nodes {
            let row = &indices[indptr[node] as usize..indptr[node + 1] as usize];
            if let Some(&neighbour) = row.iter().find(|&&v| v as usize >= n_nodes) {
                return Err(CsrError::NeighbourOutOfRange {
                    node,
                    neighbour,
                    n_nodes,
                });
            }
        }
        sort_rows(&indptr, &mut indices);

        let network = Self { indptr, indices };
        network.check_symmetry()?;
        Ok(network)
    }

    pub fn n_nodes(&self) -> usize {
        self.indptr.len() - 1
    }

    /// Stored neighbour entries: twice the edges, once for a self-loop.
    pub fn n_entries(&self) -> usize {
        self.indices.len()
    }

    /// Undirected edges, self-loops and repeats included.
    pub fn n_edges(&self) -> usize {
        let loops: usize = (0..self.n_nodes())
            .map(|node| count(self.neighbours(node), node as u32))
            .sum();
        (self.n_entries() - loops) / 2 + loops
    }

    /// The neighbours of `node`, in increasing order.
    pub fn neighbours(&self, node: usize) -> &[u32] {
        &self.indices[self.indptr[node] as usize..self.indptr[node + 1] as usize]
    }

    /// The number of neighbour entries of `node`.
    pub fn degree(&self, node: usize) -> usize {
        (self.indptr[node + 1] - self.indptr[node]) as usize
    }

    /// Every node's degree, in node order: the `degree` column of the nodes
    /// file.
    pub fn degrees(&self) -> Vec<u32> {
        self.indptr.windows(2).map(|w| w[1] - w[0]).collect()
    }

    /// `n_nodes + 1` offsets, from 0 to [`Csr::n_entries`].
    pub fn indptr(&self) -> &[u32] {
        &self.indptr
    }

    /// Every row's neighbours, one row after the other: the `indices` column of
    /// the edges file.
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Each undirected edge once, as `(a, b)` with `a <= b`, in lexicographic
    /// order — the edge list step 1 used to write.
    ///
    /// Row `a` is sorted, so its neighbours from `a` on are exactly the edges
    /// whose smaller end is `a`, already in order.
    pub fn pairs(&self) -> Vec<Pair> {
        let mut pairs = Vec::with_capacity(self.n_edges());
        for node in 0..self.n_nodes() {
            let a = node as u32;
            let row = self.neighbours(node);
            let from = row.partition_point(|&b| b < a);
            pairs.extend(row[from..].iter().map(|&b| (a, b)));
        }
        pairs
    }

    /// Every edge `{a, b}` listed as often from `a` as from `b`.
    ///
    /// Rows are sorted, so a row's runs of equal neighbours are contiguous and
    /// the mirror's count is two binary searches away.
    fn check_symmetry(&self) -> Result<(), CsrError> {
        for node in 0..self.n_nodes() {
            let a = node as u32;
            let row = self.neighbours(node);
            let mut at = 0;
            while at < row.len() {
                let b = row[at];
                let forward = row[at..].partition_point(|&v| v == b);
                if b != a {
                    let backward = count(self.neighbours(b as usize), a);
                    if forward != backward {
                        return Err(CsrError::Asymmetric {
                            a,
                            b,
                            forward,
                            backward,
                        });
                    }
                }
                at += forward;
            }
        }
        Ok(())
    }
}

/// Offsets from degrees, refusing a total past what `u32` offsets can hold.
fn offsets(degrees: impl Iterator<Item = u64>) -> Result<Vec<u32>, CsrError> {
    let mut indptr = vec![0u32];
    let mut running = 0u64;
    for degree in degrees {
        running += degree;
        let offset = u32::try_from(running).map_err(|_| CsrError::TooLarge { entries: running })?;
        indptr.push(offset);
    }
    Ok(indptr)
}

fn sort_rows(indptr: &[u32], indices: &mut [u32]) {
    for w in indptr.windows(2) {
        indices[w[0] as usize..w[1] as usize].sort_unstable();
    }
}

/// How many times `value` occurs in the sorted `row`.
fn count(row: &[u32], value: u32) -> usize {
    row.partition_point(|&v| v <= value) - row.partition_point(|&v| v < value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_and_tail() -> Csr {
        // 0–1, 0–2, 1–2, 2–3, and node 4 alone.
        Csr::from_pairs(&[(0, 1), (0, 2), (1, 2), (2, 3)], 5).unwrap()
    }

    #[test]
    fn every_edge_is_stored_from_both_ends() {
        let csr = triangle_and_tail();
        assert_eq!(csr.neighbours(0), &[1, 2]);
        assert_eq!(csr.neighbours(1), &[0, 2]);
        assert_eq!(csr.neighbours(2), &[0, 1, 3]);
        assert_eq!(csr.neighbours(3), &[2]);
    }

    #[test]
    fn the_offsets_delimit_the_rows() {
        let csr = triangle_and_tail();
        assert_eq!(csr.indptr(), &[0, 2, 4, 7, 8, 8]);
        assert_eq!(csr.indices(), &[1, 2, 0, 2, 0, 1, 3, 2]);
    }

    #[test]
    fn the_degree_is_the_length_of_the_row() {
        let csr = triangle_and_tail();
        assert_eq!(csr.degrees(), vec![2, 2, 3, 1, 0]);
        assert_eq!(csr.degree(2), 3);
    }

    #[test]
    fn an_isolated_node_has_an_empty_row() {
        let csr = triangle_and_tail();
        assert!(csr.neighbours(4).is_empty());
        assert_eq!(csr.degree(4), 0);
    }

    #[test]
    fn the_counts_distinguish_edges_from_entries() {
        let csr = triangle_and_tail();
        assert_eq!(csr.n_nodes(), 5);
        assert_eq!(csr.n_edges(), 4);
        assert_eq!(csr.n_entries(), 8);
    }

    #[test]
    fn rows_are_sorted_whatever_order_the_pairs_came_in() {
        let csr = Csr::from_pairs(&[(3, 0), (0, 1), (2, 0)], 4).unwrap();
        assert_eq!(csr.neighbours(0), &[1, 2, 3]);
    }

    #[test]
    fn the_pairs_come_back_canonical_and_sorted() {
        let csr = Csr::from_pairs(&[(3, 2), (1, 0), (2, 0), (2, 1)], 4).unwrap();
        assert_eq!(csr.pairs(), vec![(0, 1), (0, 2), (1, 2), (2, 3)]);
    }

    /// What step 1 writes — sorted, unique, `a <= b` — comes back identical,
    /// which is what lets the network figure and the golden references see the
    /// same edge list as before.
    #[test]
    fn a_canonical_edge_list_round_trips_exactly() {
        let pairs = vec![(0, 1), (0, 2), (1, 2), (2, 3)];
        assert_eq!(Csr::from_pairs(&pairs, 5).unwrap().pairs(), pairs);
    }

    #[test]
    fn a_self_loop_is_stored_once() {
        let csr = Csr::from_pairs(&[(0, 0), (0, 1)], 2).unwrap();
        assert_eq!(csr.neighbours(0), &[0, 1]);
        assert_eq!(csr.degree(0), 2);
        assert_eq!(csr.n_entries(), 3);
        assert_eq!(csr.n_edges(), 2);
        assert_eq!(csr.pairs(), vec![(0, 0), (0, 1)]);
    }

    #[test]
    fn a_repeated_edge_is_kept_as_often_as_it_was_given() {
        let csr = Csr::from_pairs(&[(0, 1), (1, 0)], 2).unwrap();
        assert_eq!(csr.neighbours(0), &[1, 1]);
        assert_eq!(csr.n_edges(), 2);
        assert_eq!(csr.pairs(), vec![(0, 1), (0, 1)]);
    }

    #[test]
    fn an_edge_past_the_last_node_is_refused_and_named() {
        let error = Csr::from_pairs(&[(0, 1), (1, 7)], 3).unwrap_err();
        assert_eq!(
            error,
            CsrError::EdgeOutOfRange {
                a: 1,
                b: 7,
                n_nodes: 3
            }
        );
        assert!(error.to_string().contains("1–7"), "{error}");
    }

    #[test]
    fn a_network_without_edges_is_valid() {
        let csr = Csr::from_pairs(&[], 3).unwrap();
        assert_eq!(csr, Csr::empty(3));
        assert_eq!(csr.indptr(), &[0, 0, 0, 0]);
        assert!(csr.indices().is_empty());
        assert!(csr.pairs().is_empty());
    }

    #[test]
    fn a_network_without_nodes_is_valid() {
        let csr = Csr::empty(0);
        assert_eq!(csr.n_nodes(), 0);
        assert_eq!(csr.indptr(), &[0]);
        assert_eq!(csr.degrees(), Vec::<u32>::new());
    }

    // -----------------------------------------------------------------------
    // Rebuilding from the files' columns
    // -----------------------------------------------------------------------

    #[test]
    fn the_files_columns_rebuild_the_same_network() {
        let csr = triangle_and_tail();
        let rebuilt = Csr::from_degrees(&csr.degrees(), csr.indices().to_vec()).unwrap();
        assert_eq!(rebuilt, csr);
    }

    #[test]
    fn rows_written_unsorted_are_sorted_on_reading() {
        let rebuilt = Csr::from_degrees(&[2, 1, 1], vec![2, 1, 0, 0]).unwrap();
        assert_eq!(rebuilt.neighbours(0), &[1, 2]);
        assert_eq!(rebuilt, Csr::from_pairs(&[(0, 1), (0, 2)], 3).unwrap());
    }

    #[test]
    fn degrees_that_do_not_cover_the_entries_are_refused() {
        let error = Csr::from_degrees(&[1, 1], vec![1, 0, 0]).unwrap_err();
        assert_eq!(
            error,
            CsrError::LengthMismatch {
                degree_sum: 2,
                entries: 3
            }
        );
    }

    #[test]
    fn a_neighbour_past_the_last_node_is_refused() {
        let error = Csr::from_degrees(&[1, 1], vec![1, 5]).unwrap_err();
        assert_eq!(
            error,
            CsrError::NeighbourOutOfRange {
                node: 1,
                neighbour: 5,
                n_nodes: 2
            }
        );
    }

    /// Node 0 says it is linked to 1; node 1 says it is linked to 2. Read as it
    /// stands, the aggregation would walk from 0 to 1 and never back.
    #[test]
    fn an_edge_listed_from_one_end_only_is_refused() {
        let error = Csr::from_degrees(&[1, 1, 1], vec![1, 2, 1]).unwrap_err();
        assert!(
            matches!(error, CsrError::Asymmetric { a: 0, b: 1, .. }),
            "{error:?}"
        );
        assert!(error.to_string().contains("undirected"), "{error}");
    }

    #[test]
    fn a_repeated_edge_must_be_repeated_from_both_ends() {
        let error = Csr::from_degrees(&[2, 1], vec![1, 1, 0]).unwrap_err();
        assert_eq!(
            error,
            CsrError::Asymmetric {
                a: 0,
                b: 1,
                forward: 2,
                backward: 1
            }
        );
    }

    #[test]
    fn a_self_loop_reads_back_from_its_single_entry() {
        let csr = Csr::from_pairs(&[(0, 0), (0, 1)], 2).unwrap();
        let rebuilt = Csr::from_degrees(&csr.degrees(), csr.indices().to_vec()).unwrap();
        assert_eq!(rebuilt, csr);
    }
}
