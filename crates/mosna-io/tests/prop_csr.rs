//! Property tests of the compressed sparse row network and its two files.
//!
//! The format replaces a one-row-per-edge table that every step read; what
//! must hold is that nothing is lost or invented on the way through — for any
//! network, not only the ones the example tests thought of.

use mosna_io::network::{read_network, write_network, Csr, Pair};
use mosna_io::{read::get_opener::Extension, Table};
use proptest::prelude::*;

/// A network as step 1 produces one: `n` nodes and an edge list that is
/// sorted, unique, and has `a < b` in every pair.
fn canonical_network() -> impl Strategy<Value = (usize, Vec<Pair>)> {
    (2usize..40).prop_flat_map(|n| {
        // `a < b` by construction: a filter would reject almost everything on
        // the smallest networks.
        let pair = (0..n as u32 - 1).prop_flat_map(move |a| (Just(a), a + 1..n as u32));
        proptest::collection::btree_set(pair, 0..120)
            .prop_map(move |set| (n, set.into_iter().collect::<Vec<_>>()))
    })
}

/// Any edge list at all: either direction, repeats, self-loops, any order.
fn any_edge_list() -> impl Strategy<Value = (usize, Vec<Pair>)> {
    (1usize..30).prop_flat_map(|n| {
        proptest::collection::vec((0..n as u32, 0..n as u32), 0..80)
            .prop_map(move |pairs| (n, pairs))
    })
}

fn nodes(n: usize) -> Table {
    Table::from_columns(vec![(
        "x".into(),
        Table::f64_array((0..n).map(|i| i as f64)),
    )])
    .unwrap()
}

proptest! {
    /// What step 1 writes comes back as the very same edge list.
    #[test]
    fn prop_a_canonical_edge_list_round_trips((n, pairs) in canonical_network()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        prop_assert_eq!(csr.pairs(), pairs);
    }

    /// Every entry has its mirror: the network is undirected however it was
    /// given.
    #[test]
    fn prop_every_edge_is_listed_from_both_ends((n, pairs) in any_edge_list()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        for node in 0..n {
            for &other in csr.neighbours(node) {
                let forward = csr.neighbours(node).iter().filter(|&&v| v == other).count();
                let backward = csr
                    .neighbours(other as usize)
                    .iter()
                    .filter(|&&v| v as usize == node)
                    .count();
                prop_assert_eq!(forward, backward);
            }
        }
    }

    /// Each row is sorted, so a neighbourhood reads the same on every machine.
    #[test]
    fn prop_rows_are_sorted((n, pairs) in any_edge_list()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        for node in 0..n {
            prop_assert!(csr.neighbours(node).windows(2).all(|w| w[0] <= w[1]));
        }
    }

    /// The offsets start at 0, never decrease, and end on the last entry.
    #[test]
    fn prop_the_offsets_are_consistent((n, pairs) in any_edge_list()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        let indptr = csr.indptr();
        prop_assert_eq!(indptr.len(), n + 1);
        prop_assert_eq!(indptr[0], 0);
        prop_assert!(indptr.windows(2).all(|w| w[0] <= w[1]));
        prop_assert_eq!(indptr[n] as usize, csr.indices().len());
    }

    /// An edge is two entries, a self-loop one; and the edges are all there.
    #[test]
    fn prop_the_counts_add_up((n, pairs) in any_edge_list()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        let loops = pairs.iter().filter(|(a, b)| a == b).count();
        prop_assert_eq!(csr.n_edges(), pairs.len());
        prop_assert_eq!(csr.n_entries(), 2 * pairs.len() - loops);
        prop_assert_eq!(
            csr.degrees().iter().map(|&d| d as usize).sum::<usize>(),
            csr.n_entries()
        );
    }

    /// The edge list it gives back is the input, normalised: each pair as
    /// `(min, max)`, sorted, repeats kept.
    #[test]
    fn prop_the_pairs_are_the_input_normalised((n, pairs) in any_edge_list()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        let mut expected: Vec<Pair> = pairs.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
        expected.sort_unstable();
        prop_assert_eq!(csr.pairs(), expected);
    }

    /// The two columns the files hold rebuild the network exactly.
    #[test]
    fn prop_the_columns_rebuild_the_network((n, pairs) in any_edge_list()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        let rebuilt = Csr::from_degrees(&csr.degrees(), csr.indices().to_vec()).unwrap();
        prop_assert_eq!(rebuilt, csr);
    }

    /// Removing a single entry breaks the symmetry, and is caught.
    #[test]
    fn prop_a_missing_mirror_entry_is_caught(
        (n, pairs) in canonical_network(),
        pick in any::<prop::sample::Index>(),
    ) {
        prop_assume!(!pairs.is_empty());
        let csr = Csr::from_pairs(&pairs, n).unwrap();
        let mut degrees = csr.degrees();
        let mut indices = csr.indices().to_vec();

        let entry = pick.index(indices.len());
        let node = csr.indptr().partition_point(|&start| start as usize <= entry) - 1;
        indices.remove(entry);
        degrees[node] -= 1;

        prop_assert!(Csr::from_degrees(&degrees, indices).is_err());
    }

    /// Written as tables, read back: the same network, in either format.
    #[test]
    fn prop_both_formats_read_as_the_same_network((n, pairs) in any_edge_list()) {
        let csr = Csr::from_pairs(&pairs, n).unwrap();

        let csr_nodes = nodes(n).with_degrees(&csr).unwrap();
        let csr_edges = Table::csr_edges(&csr).unwrap();
        prop_assert_eq!(read_network(&csr_nodes, &csr_edges).unwrap(), csr.clone());

        let legacy_edges = Table::source_target_edges(&pairs).unwrap();
        prop_assert_eq!(read_network(&nodes(n), &legacy_edges).unwrap(), csr);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// Through parquet on disk, which is where the steps hand networks over.
    #[test]
    fn prop_a_network_survives_its_files((n, pairs) in canonical_network()) {
        let dir = tempfile::tempdir().unwrap();
        let nodes_path = dir.path().join("nodes_patient-1.parquet");
        let edges_path = dir.path().join("edges_patient-1.parquet");
        let csr = Csr::from_pairs(&pairs, n).unwrap();

        write_network(&nodes(n), &csr, &nodes_path, &edges_path).unwrap();
        let (_, read) = mosna_io::network::read_network_files(
            &nodes_path,
            &edges_path,
            Extension::Parquet,
        )
        .unwrap();
        prop_assert_eq!(read, csr);
    }
}
