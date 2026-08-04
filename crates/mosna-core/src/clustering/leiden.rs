//! The Leiden community detection algorithm.
//!
//! Replaces `leidenalg.find_partition(G, la.RBConfigurationVertexPartition,
//! resolution_parameter=resolution, seed=0)` in `clustering.py::get_clusterer`.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::clustering::relabel_clusters::relabel_clusters;

/// A weighted undirected graph, with self-loops kept separately.
///
/// Aggregation folds each community into a single node whose self-loop carries
/// the community's internal weight, so self-loops are not a corner case here —
/// they are how the recursion works.
#[derive(Debug, Clone)]
struct Graph {
    n_nodes: usize,
    /// `adj[i]` holds `(neighbour, weight)`, never `i` itself.
    adj: Vec<Vec<(usize, f64)>>,
    /// Internal weight of node `i`, counted once.
    self_loop: Vec<f64>,
    /// `sum_j A_ij`, with the self-loop counted twice.
    degree: Vec<f64>,
    /// `sum_i degree[i]`.
    two_m: f64,
}

impl Graph {
    fn new(n_nodes: usize, edges: &[(usize, usize, f64)]) -> Self {
        let mut adj = vec![Vec::new(); n_nodes];
        let mut self_loop = vec![0.0f64; n_nodes];

        for &(a, b, w) in edges {
            if a >= n_nodes || b >= n_nodes || w <= 0.0 {
                continue;
            }
            if a == b {
                self_loop[a] += w;
            } else {
                adj[a].push((b, w));
                adj[b].push((a, w));
            }
        }

        let degree: Vec<f64> = (0..n_nodes)
            .map(|i| adj[i].iter().map(|&(_, w)| w).sum::<f64>() + 2.0 * self_loop[i])
            .collect();
        let two_m = degree.iter().sum();

        Self {
            n_nodes,
            adj,
            self_loop,
            degree,
            two_m,
        }
    }
}

/// Partition a graph into communities.
///
/// `edges` is an undirected weighted edge list; `resolution` is the `gamma` of
/// the RBConfiguration objective
///
/// ```text
/// Q = sum_ij [ A_ij - gamma * k_i k_j / 2m ] delta(c_i, c_j)
/// ```
///
/// A higher `gamma` penalises large communities and so yields more of them,
/// which is what the `resolution` setting in the configuration controls.
///
/// The algorithm is Leiden proper, not Louvain: after each local-moving pass
/// the communities are *refined* into well-connected sub-communities before
/// being aggregated. That refinement is what stops Louvain's known failure
/// mode, where a community can end up internally disconnected and yet never be
/// split again.
///
/// Labels come back renumbered contiguously from 0.
pub fn leiden(
    n_nodes: usize,
    edges: &[(usize, usize, f64)],
    resolution: f64,
    seed: u64,
) -> Vec<u32> {
    if n_nodes == 0 {
        return Vec::new();
    }

    let mut graph = Graph::new(n_nodes, edges);
    // Maps each original node to its node id in the *current* graph. Every
    // aggregation rewrites it; the community assignment is only applied at the
    // very end, because the two numberings are different and mixing them up
    // collapses the whole partition.
    let mut membership: Vec<usize> = (0..n_nodes).collect();
    // Partition of the current (possibly aggregated) graph.
    let mut partition: Vec<usize> = (0..n_nodes).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    for _ in 0..64 {
        let improved = local_move(&graph, &mut partition, resolution, &mut rng);
        let n_communities = renumber(&mut partition);

        if !improved || n_communities == graph.n_nodes {
            break;
        }

        // Refine each community into well-connected pieces, then aggregate on
        // the refined partition while keeping the coarse communities as the
        // starting point of the next level.
        let mut refined = refine(&graph, &partition, resolution, &mut rng);
        let n_refined = renumber(&mut refined);

        let coarse: Vec<usize> = {
            // For each refined community, the coarse community it came from.
            let mut map = vec![0usize; n_refined];
            for node in 0..graph.n_nodes {
                map[refined[node]] = partition[node];
            }
            map
        };

        for node in membership.iter_mut() {
            *node = refined[*node];
        }
        graph = aggregate(&graph, &refined, n_refined);
        partition = coarse;
    }

    // Now — and only now — turn node ids into community ids.
    let communities: Vec<u32> = membership
        .iter()
        .map(|&node| partition[node] as u32)
        .collect();
    relabel_clusters(&communities)
}

/// Move nodes one at a time into the neighbouring community that improves the
/// objective most, until no move helps.
fn local_move(
    graph: &Graph,
    partition: &mut [usize],
    resolution: f64,
    rng: &mut ChaCha8Rng,
) -> bool {
    if graph.two_m <= 0.0 {
        return false;
    }

    // Total degree of each community.
    let mut community_degree = vec![0.0f64; graph.n_nodes];
    for node in 0..graph.n_nodes {
        community_degree[partition[node]] += graph.degree[node];
    }

    let mut order: Vec<usize> = (0..graph.n_nodes).collect();
    order.shuffle(rng);

    let mut any_improvement = false;
    // A `BTreeMap`, not a `HashMap`: the loop below breaks ties by iteration
    // order, and `HashMap` seeds its hasher per thread — the same graph would
    // then be partitioned differently on a worker thread than on the main one,
    // and differently again in the next process. Ordering by community id makes
    // the tie-break a property of the graph. The map holds one entry per
    // neighbouring community, so it is tiny and the tree costs nothing.
    let mut weights_to: std::collections::BTreeMap<usize, f64> = Default::default();

    for _ in 0..32 {
        let mut moved = false;

        for &node in &order {
            let current = partition[node];

            weights_to.clear();
            for &(neighbour, w) in &graph.adj[node] {
                *weights_to.entry(partition[neighbour]).or_insert(0.0) += w;
            }

            // Removing the node from its own community first, so the gain of
            // staying is measured on the same footing as the gain of leaving.
            community_degree[current] -= graph.degree[node];

            let mut best_community = current;
            let mut best_gain = weights_to.get(&current).copied().unwrap_or(0.0)
                - resolution * graph.degree[node] * community_degree[current] / graph.two_m;

            for (&candidate, &weight) in &weights_to {
                if candidate == current {
                    continue;
                }
                let gain = weight
                    - resolution * graph.degree[node] * community_degree[candidate] / graph.two_m;
                if gain > best_gain + 1e-12 {
                    best_gain = gain;
                    best_community = candidate;
                }
            }

            community_degree[best_community] += graph.degree[node];
            if best_community != current {
                partition[node] = best_community;
                moved = true;
                any_improvement = true;
            }
        }

        if !moved {
            break;
        }
    }

    any_improvement
}

/// Split each community into well-connected sub-communities.
///
/// Starts from singletons inside the community and merges greedily, never
/// crossing a community boundary. A community that is internally disconnected
/// comes out as several pieces, which is precisely what Louvain cannot do and
/// Leiden can.
fn refine(graph: &Graph, partition: &[usize], resolution: f64, rng: &mut ChaCha8Rng) -> Vec<usize> {
    let mut refined: Vec<usize> = (0..graph.n_nodes).collect();
    let mut community_degree: Vec<f64> = graph.degree.clone();

    let mut order: Vec<usize> = (0..graph.n_nodes).collect();
    order.shuffle(rng);

    // A `BTreeMap`, not a `HashMap`: the loop below breaks ties by iteration
    // order, and `HashMap` seeds its hasher per thread — the same graph would
    // then be partitioned differently on a worker thread than on the main one,
    // and differently again in the next process. Ordering by community id makes
    // the tie-break a property of the graph. The map holds one entry per
    // neighbouring community, so it is tiny and the tree costs nothing.
    let mut weights_to: std::collections::BTreeMap<usize, f64> = Default::default();

    for &node in &order {
        weights_to.clear();
        for &(neighbour, w) in &graph.adj[node] {
            // The refinement may not merge across the coarse communities.
            if partition[neighbour] != partition[node] {
                continue;
            }
            *weights_to.entry(refined[neighbour]).or_insert(0.0) += w;
        }
        if weights_to.is_empty() {
            continue;
        }

        let current = refined[node];
        community_degree[current] -= graph.degree[node];

        let mut best_community = current;
        let mut best_gain = 0.0f64;
        for (&candidate, &weight) in &weights_to {
            if candidate == current {
                continue;
            }
            let gain = weight
                - resolution * graph.degree[node] * community_degree[candidate] / graph.two_m;
            if gain > best_gain + 1e-12 {
                best_gain = gain;
                best_community = candidate;
            }
        }

        community_degree[best_community] += graph.degree[node];
        refined[node] = best_community;
    }

    refined
}

/// Fold each community into a single node.
fn aggregate(graph: &Graph, partition: &[usize], n_communities: usize) -> Graph {
    // Ordered, because the adjacency lists are built by iterating it: a
    // hash order would make the next level's floating-point sums associate
    // differently, and the whole partition would follow.
    let mut between: std::collections::BTreeMap<(usize, usize), f64> = Default::default();
    let mut self_loop = vec![0.0f64; n_communities];

    for node in 0..graph.n_nodes {
        // Internal weight of the node carries over into its community.
        self_loop[partition[node]] += graph.self_loop[node];

        for &(neighbour, w) in &graph.adj[node] {
            let (a, b) = (partition[node], partition[neighbour]);
            if a == b {
                // Each internal edge is seen twice, once from each endpoint.
                self_loop[a] += w / 2.0;
            } else if node < neighbour {
                let key = if a < b { (a, b) } else { (b, a) };
                *between.entry(key).or_insert(0.0) += w;
            }
        }
    }

    let mut adj = vec![Vec::new(); n_communities];
    for ((a, b), w) in between {
        adj[a].push((b, w));
        adj[b].push((a, w));
    }

    let degree: Vec<f64> = (0..n_communities)
        .map(|i| adj[i].iter().map(|&(_, w)| w).sum::<f64>() + 2.0 * self_loop[i])
        .collect();
    let two_m = degree.iter().sum();

    Graph {
        n_nodes: n_communities,
        adj,
        self_loop,
        degree,
        two_m,
    }
}

/// Renumber a partition contiguously in place, returning the community count.
fn renumber(partition: &mut [usize]) -> usize {
    let mut mapping: std::collections::HashMap<usize, usize> = Default::default();
    for label in partition.iter_mut() {
        let next = mapping.len();
        *label = *mapping.entry(*label).or_insert(next);
    }
    mapping.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n_communities(labels: &[u32]) -> usize {
        let mut d = labels.to_vec();
        d.sort_unstable();
        d.dedup();
        d.len()
    }

    /// A seeded algorithm that answers differently on two threads is not
    /// seeded at all.
    ///
    /// `std::collections::HashMap` seeds its hasher per thread, so anything
    /// that iterates one and lets the order decide a tie will give a different
    /// partition on a rayon worker than on the main thread — and a different
    /// one again in the next process. This test is what pins that shut.
    #[test]
    fn the_partition_does_not_depend_on_the_thread_it_runs_on() {
        let (n_nodes, edges) = three_cliques();

        let here = leiden(n_nodes, &edges, 1.0, 42);
        let there = std::thread::spawn(move || leiden(n_nodes, &edges, 1.0, 42))
            .join()
            .unwrap();

        assert_eq!(here, there, "the partition depends on the thread");
    }

    /// The same, on a graph with many tied gains — the case where the
    /// iteration order actually decides something.
    #[test]
    fn a_graph_full_of_ties_is_partitioned_the_same_on_any_thread() {
        // A ring: every neighbouring community offers exactly the same gain,
        // so every choice is a tie.
        let n_nodes = 60;
        let edges: Vec<(usize, usize, f64)> =
            (0..n_nodes).map(|i| (i, (i + 1) % n_nodes, 1.0)).collect();

        let here = leiden(n_nodes, &edges, 1.0, 7);
        let there = std::thread::spawn(move || leiden(n_nodes, &edges, 1.0, 7))
            .join()
            .unwrap();

        assert_eq!(here, there);
    }

    /// Three six-node cliques, chained by two single edges.
    fn three_cliques() -> (usize, Vec<(usize, usize, f64)>) {
        let mut edges = Vec::new();
        for clique in 0..3usize {
            for i in 0..6 {
                for j in (i + 1)..6 {
                    edges.push((clique * 6 + i, clique * 6 + j, 1.0));
                }
            }
        }
        edges.push((5, 6, 1.0));
        edges.push((11, 12, 1.0));
        (18, edges)
    }

    #[test]
    fn recovers_three_cliques() {
        let (n, edges) = three_cliques();
        let labels = leiden(n, &edges, 1.0, 0);
        assert_eq!(n_communities(&labels), 3);
        for clique in 0..3 {
            let first = labels[clique * 6];
            assert!(
                labels[clique * 6..(clique + 1) * 6]
                    .iter()
                    .all(|&l| l == first),
                "clique {clique} was split: {labels:?}"
            );
        }
    }

    #[test]
    fn a_higher_resolution_never_yields_fewer_communities() {
        let (n, edges) = three_cliques();
        let coarse = n_communities(&leiden(n, &edges, 0.1, 0));
        let fine = n_communities(&leiden(n, &edges, 8.0, 0));
        assert!(fine >= coarse, "{coarse} -> {fine}");
    }

    #[test]
    fn isolated_nodes_each_form_their_own_community() {
        let labels = leiden(5, &[], 1.0, 0);
        assert_eq!(n_communities(&labels), 5);
    }

    #[test]
    fn a_single_clique_stays_whole() {
        let mut edges = Vec::new();
        for i in 0..8 {
            for j in (i + 1)..8 {
                edges.push((i, j, 1.0));
            }
        }
        assert_eq!(n_communities(&leiden(8, &edges, 1.0, 0)), 1);
    }

    #[test]
    fn the_result_is_reproducible() {
        let (n, edges) = three_cliques();
        assert_eq!(leiden(n, &edges, 1.0, 3), leiden(n, &edges, 1.0, 3));
    }

    #[test]
    fn labels_are_contiguous() {
        let (n, edges) = three_cliques();
        let labels = leiden(n, &edges, 1.0, 0);
        let mut distinct = labels.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct, (0..distinct.len() as u32).collect::<Vec<_>>());
    }

    #[test]
    fn edge_weights_are_honoured() {
        // Two pairs, joined weakly. The heavy edges must define the split.
        let edges = vec![(0, 1, 10.0), (2, 3, 10.0), (1, 2, 0.01)];
        let labels = leiden(4, &edges, 1.0, 0);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[2], labels[3]);
        assert_ne!(labels[0], labels[2]);
    }

    #[test]
    fn handles_an_empty_graph() {
        assert!(leiden(0, &[], 1.0, 0).is_empty());
    }

    #[test]
    fn ignores_out_of_range_and_non_positive_edges() {
        let edges = vec![(0, 99, 1.0), (0, 1, -5.0), (0, 1, 1.0)];
        let labels = leiden(2, &edges, 1.0, 0);
        assert_eq!(labels.len(), 2);
    }
}
