//! The spectral initialisation of UMAP — umap-learn's `init='spectral'`.
//!
//! A port of `umap/spectral.py` as umap-learn 0.5.12 ships it:
//! `spectral_layout`, `multi_component_layout` and `component_layout`, and of
//! the `SpectralEmbedding` of scikit-learn that the last one calls.
//!
//! # The idea
//!
//! The starting layout is the eigenvectors of the normalised Laplacian of the
//! fuzzy graph,
//!
//! ```text
//! L = I - D^(-1/2) W D^(-1/2)
//! ```
//!
//! for its smallest eigenvalues, the first — the trivial `sqrt(degree)` — left
//! out. Those eigenvectors are the smoothest functions on the graph: two
//! points joined by heavy edges get close coordinates. It is a Laplacian
//! eigenmap of the very graph the layout will then be optimised against, which
//! is why it is umap-learn's default.
//!
//! The smallest eigenvalues of `L` are `1 - mu` for the largest eigenvalues
//! `mu` of `D^(-1/2) W D^(-1/2)`; that operator is what is handed to
//! [`fn@leading_eigenpairs`], applied edge by edge, never formed.
//!
//! # A graph in several pieces
//!
//! A disconnected graph has one trivial eigenvector per component, and the
//! eigenvectors say nothing about how the components sit relative to one
//! another. umap-learn then lays out each component on its own and places them
//! by a second, small spectral embedding of their centroids in the data space
//! — reproduced here step for step. On the NAS features, where identical rows
//! are common, the graph does fall apart into components, so this is not a
//! corner case.
//!
//! # Where this departs from umap-learn
//!
//! * The eigensolver. umap-learn calls ARPACK (`eigsh`, `tol=1e-4`) below two
//!   million points and LOBPCG above; this is LOBPCG throughout, to the same
//!   tolerance on the residual. Both return the same eigenvectors up to sign
//!   and to that tolerance.
//! * Non-convergence. umap-learn falls back to a random layout when ARPACK
//!   fails. The block LOBPCG ends on after `MAX_ITER` iterations is already
//!   close to the eigenvectors, so it is kept rather than thrown away; a
//!   random layout is used only when the solver produced nothing usable.
//! * The random streams are this crate's seeded ones, not numpy's.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use crate::linalg::eigen::symmetric_eigen;
use crate::linalg::lobpcg::leading_eigenpairs;
use crate::reduction::umap::metric::Metric;
use crate::reduction::umap::Scalar;

/// Residual tolerance of the eigensolver — `tol=1e-4` in umap-learn.
const TOL: f64 = 1e-4;

/// Iteration cap of the eigensolver. scikit-learn caps its own LOBPCG at 2000.
const MAX_ITER: usize = 2000;

/// Below this many nodes the eigenproblem is solved densely, exactly.
const DENSE_MAX: usize = 300;

/// The raw spectral layout of the graph `edges` on `n_rows` points: one row per
/// point, `dim` columns, row-major.
///
/// `edges` is the fuzzy simplicial set already pruned of the edges too weak to
/// be sampled, as umap-learn prunes it before initialising. `data` is the input
/// matrix, read only to place the components of a disconnected graph.
///
/// The coordinates are those of `spectral_layout`, before the
/// `noisy_scale_coords` the caller applies.
#[allow(clippy::too_many_arguments)]
pub fn spectral_layout(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    edges: &[(usize, usize, Scalar)],
    dim: usize,
    metric: Metric,
    seed: u64,
) -> Vec<f64> {
    let adjacency = Adjacency::new(n_rows, edges);
    let (labels, n_components) = connected_components(&adjacency);
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0x5bd1_e995_7f4a_7c15);

    if n_components == 1 {
        let nodes: Vec<usize> = (0..n_rows).collect();
        return match laplacian_eigenmap(&adjacency, &nodes, dim, seed) {
            Some(layout) => layout,
            // `gen.uniform(low=-10.0, high=10.0, size=(n, dim))`.
            None => (0..n_rows * dim)
                .map(|_| rng.gen_range(-10.0..10.0))
                .collect(),
        };
    }

    multi_component_layout(
        data,
        n_features,
        &adjacency,
        &labels,
        n_components,
        dim,
        metric,
        seed,
        &mut rng,
    )
}

/// `multi_component_layout`: each component on its own, placed by the
/// embedding of the components themselves.
#[allow(clippy::too_many_arguments)]
fn multi_component_layout(
    data: &[Scalar],
    n_features: usize,
    adjacency: &Adjacency,
    labels: &[usize],
    n_components: usize,
    dim: usize,
    metric: Metric,
    seed: u64,
    rng: &mut ChaCha8Rng,
) -> Vec<f64> {
    let n_rows = labels.len();
    let meta = if n_components > 2 * dim {
        component_layout(data, n_features, labels, n_components, dim, metric, seed)
    } else {
        // `base = hstack([eye(k), zeros((k, dim - k))])`, then `vstack([base,
        // -base])[:n_components]`: the components on the axes, two per axis.
        let k = n_components.div_ceil(2);
        let mut meta = vec![0.0f64; n_components * dim];
        for c in 0..n_components {
            let (axis, sign) = if c < k { (c, 1.0) } else { (c - k, -1.0) };
            if axis < dim {
                meta[c * dim + axis] = sign;
            }
        }
        meta
    };

    let mut members: Vec<Vec<usize>> = vec![Vec::new(); n_components];
    for (node, &label) in labels.iter().enumerate() {
        members[label].push(node);
    }

    // The components are independent eigenproblems, each with its own seed:
    // they are solved side by side, and assembled below in label order.
    let embeddings: Vec<Option<Vec<f64>>> = members
        .par_iter()
        .enumerate()
        .map(|(label, nodes)| {
            let size = nodes.len();
            if size < 2 * dim || size <= dim + 1 {
                None
            } else {
                laplacian_eigenmap(adjacency, nodes, dim, seed.wrapping_add(label as u64 + 1))
            }
        })
        .collect();

    let mut result = vec![0.0f64; n_rows * dim];
    for (label, (nodes, embedded)) in members.iter().zip(embeddings).enumerate() {
        let centre = &meta[label * dim..(label + 1) * dim];

        // Half the distance to the nearest other component.
        let nearest = (0..n_components)
            .map(|other| euclidean(centre, &meta[other * dim..(other + 1) * dim]))
            .filter(|&d| d > 0.0)
            .fold(f64::INFINITY, f64::min);
        let data_range = if nearest.is_finite() {
            nearest / 2.0
        } else {
            1.0
        };

        match embedded {
            Some(layout) => {
                let extent = layout.iter().fold(0.0f64, |m, v| m.max(v.abs()));
                let expansion = if extent > 0.0 {
                    data_range / extent
                } else {
                    0.0
                };
                for (local, &node) in nodes.iter().enumerate() {
                    for d in 0..dim {
                        result[node * dim + d] = layout[local * dim + d] * expansion + centre[d];
                    }
                }
            }
            None => {
                for &node in nodes {
                    for d in 0..dim {
                        result[node * dim + d] =
                            rng.gen_range(-data_range..=data_range) + centre[d];
                    }
                }
            }
        }
    }
    result
}

/// `component_layout`: a spectral embedding of the components' centroids, on
/// the affinity `exp(-d^2)` between them.
fn component_layout(
    data: &[Scalar],
    n_features: usize,
    labels: &[usize],
    n_components: usize,
    dim: usize,
    metric: Metric,
    seed: u64,
) -> Vec<f64> {
    // Centroids in the data space.
    let mut centroids = vec![0.0f64; n_components * n_features];
    let mut counts = vec![0usize; n_components];
    for (row, &label) in labels.iter().enumerate() {
        counts[label] += 1;
        for f in 0..n_features {
            centroids[label * n_features + f] += data[row * n_features + f] as f64;
        }
    }
    for (label, &count) in counts.iter().enumerate() {
        for f in 0..n_features {
            centroids[label * n_features + f] /= count.max(1) as f64;
        }
    }

    // `exp(-(distance_matrix ** 2))`, its diagonal left out as
    // `scipy.sparse.csgraph.laplacian` leaves it out.
    let centroid = |c: usize| &centroids[c * n_features..(c + 1) * n_features];
    let affinity: Vec<f64> = (0..n_components * n_components)
        .into_par_iter()
        .map(|cell| {
            let (i, j) = (cell / n_components, cell % n_components);
            if i == j {
                0.0
            } else {
                let d = metric_f64(metric, centroid(i), centroid(j));
                (-(d * d)).exp()
            }
        })
        .collect();

    let embedding = spectral_embedding(&affinity, n_components, dim, seed);

    // `component_embedding /= component_embedding.max()`.
    let max = embedding.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max.is_finite() && max != 0.0 {
        embedding.iter().map(|v| v / max).collect()
    } else {
        embedding
    }
}

/// scikit-learn's `spectral_embedding(affinity, n_components=dim,
/// norm_laplacian=True, drop_first=True)` on a dense affinity, row-major
/// `n x dim`.
fn spectral_embedding(affinity: &[f64], n: usize, dim: usize, seed: u64) -> Vec<f64> {
    // `csgraph_laplacian(normed=True, return_diag=True)`: an isolated node
    // gets a degree of one and a zero row.
    let degree: Vec<f64> = (0..n)
        .map(|i| affinity[i * n..(i + 1) * n].iter().sum::<f64>())
        .collect();
    let isolated: Vec<bool> = degree.iter().map(|&w| w == 0.0).collect();
    let dd: Vec<f64> = degree
        .iter()
        .zip(&isolated)
        .map(|(&w, &iso)| if iso { 1.0 } else { w.sqrt() })
        .collect();

    // The smallest eigenvalues of `L` are the largest of `I - L`, which is
    // `D^(-1/2) A D^(-1/2)` with a one on the diagonal of an isolated node.
    let operator = |x: &[f64], y: &mut [f64]| {
        y.par_iter_mut().enumerate().for_each(|(i, out)| {
            *out = if isolated[i] {
                x[i]
            } else {
                let row = &affinity[i * n..(i + 1) * n];
                let mut sum = 0.0;
                for j in 0..n {
                    if row[j] != 0.0 {
                        sum += row[j] * x[j] / dd[j];
                    }
                }
                sum / dd[i]
            };
        });
    };

    let k = dim + 1;
    let vectors = leading_vectors(n, k, &operator, &dd, seed);
    let Some(vectors) = vectors else {
        return vec![0.0; n * dim];
    };

    // `embedding / dd`, then `_deterministic_vector_sign_flip`, then the first
    // dropped.
    let mut embedding = vec![0.0f64; n * dim];
    for (d, vector) in vectors.iter().skip(1).take(dim).enumerate() {
        let scaled: Vec<f64> = vector.iter().zip(&dd).map(|(v, s)| v / s).collect();
        let pivot = scaled
            .iter()
            .copied()
            .fold(0.0f64, |m, v| if v.abs() > m.abs() { v } else { m });
        let sign = if pivot < 0.0 { -1.0 } else { 1.0 };
        for i in 0..n {
            embedding[i * dim + d] = sign * scaled[i];
        }
    }
    embedding
}

/// The `dim` eigenvectors of the normalised Laplacian of one connected
/// component after the trivial one, row-major `nodes.len() x dim`.
///
/// `_spectral_layout` on a connected graph: `k = dim + 1` eigenpairs, the
/// first — `sqrt(degree)`, which also seeds the solver as
/// `X[:, 0] = sqrt_deg / norm(sqrt_deg)` does — dropped.
fn laplacian_eigenmap(
    adjacency: &Adjacency,
    nodes: &[usize],
    dim: usize,
    seed: u64,
) -> Option<Vec<f64>> {
    let m = nodes.len();
    // Local numbering of the component.
    let mut local = std::collections::HashMap::with_capacity(m);
    for (index, &node) in nodes.iter().enumerate() {
        local.insert(node, index);
    }
    let neighbours: Vec<Vec<(usize, f64)>> = nodes
        .iter()
        .map(|&node| {
            adjacency
                .neighbours(node)
                .iter()
                .filter_map(|&(other, w)| local.get(&other).map(|&j| (j, w)))
                .collect()
        })
        .collect();
    let sqrt_degree: Vec<f64> = neighbours
        .iter()
        .map(|row| row.iter().map(|&(_, w)| w).sum::<f64>().sqrt())
        .collect();
    if sqrt_degree.iter().any(|&s| s.is_nan() || s <= 0.0) {
        return None;
    }

    let operator = |x: &[f64], y: &mut [f64]| {
        y.par_iter_mut()
            .with_min_len(4096)
            .enumerate()
            .for_each(|(i, out)| {
                let mut sum = 0.0;
                for &(j, w) in &neighbours[i] {
                    sum += w * x[j] / sqrt_degree[j];
                }
                *out = sum / sqrt_degree[i];
            });
    };

    let vectors = leading_vectors(m, dim + 1, &operator, &sqrt_degree, seed)?;
    if vectors.len() < dim + 1 {
        return None;
    }
    let mut layout = vec![0.0f64; m * dim];
    for (d, vector) in vectors.iter().skip(1).take(dim).enumerate() {
        for i in 0..m {
            layout[i * dim + d] = vector[i];
        }
    }
    layout.iter().all(|v| v.is_finite()).then_some(layout)
}

/// The `k` leading eigenvectors of a symmetric operator on `n` points, largest
/// eigenvalue first: densely below [`DENSE_MAX`], by LOBPCG above, seeded with
/// `trivial` — the known leading eigenvector — as umap-learn seeds its solver.
fn leading_vectors(
    n: usize,
    k: usize,
    operator: &(dyn Fn(&[f64], &mut [f64]) + Sync),
    trivial: &[f64],
    seed: u64,
) -> Option<Vec<Vec<f64>>> {
    if n < k {
        return None;
    }
    if n <= DENSE_MAX {
        // Column by column, from the operator itself.
        let mut matrix = vec![0.0f64; n * n];
        let mut unit = vec![0.0f64; n];
        let mut column = vec![0.0f64; n];
        for j in 0..n {
            unit[j] = 1.0;
            operator(&unit, &mut column);
            unit[j] = 0.0;
            for i in 0..n {
                matrix[i * n + j] = column[i];
            }
        }
        for i in 0..n {
            for j in (i + 1)..n {
                let value = 0.5 * (matrix[i * n + j] + matrix[j * n + i]);
                matrix[i * n + j] = value;
                matrix[j * n + i] = value;
            }
        }
        let eigen = symmetric_eigen(&matrix, n);
        return Some(
            (0..k)
                .map(|c| {
                    let source = n - 1 - c;
                    (0..n).map(|i| eigen.vectors[i * n + source]).collect()
                })
                .collect(),
        );
    }

    let norm = trivial.iter().map(|v| v * v).sum::<f64>().sqrt();
    let start: Vec<Vec<f64>> = if norm > 0.0 {
        vec![trivial.iter().map(|v| v / norm).collect()]
    } else {
        Vec::new()
    };
    let found = leading_eigenpairs(n, k, operator, &start, seed, TOL, MAX_ITER)?;
    (found.vectors.len() == k).then_some(found.vectors)
}

/// The fuzzy graph as adjacency lists.
struct Adjacency {
    rows: Vec<Vec<(usize, f64)>>,
}

impl Adjacency {
    fn new(n_rows: usize, edges: &[(usize, usize, Scalar)]) -> Self {
        let mut rows = vec![Vec::new(); n_rows];
        for &(a, b, w) in edges {
            if a == b || w <= 0.0 {
                continue;
            }
            rows[a].push((b, w as f64));
            rows[b].push((a, w as f64));
        }
        Self { rows }
    }

    fn neighbours(&self, node: usize) -> &[(usize, f64)] {
        &self.rows[node]
    }
}

/// Component labels numbered in order of first appearance, as
/// `scipy.sparse.csgraph.connected_components` numbers them, and their count.
fn connected_components(adjacency: &Adjacency) -> (Vec<usize>, usize) {
    let n = adjacency.rows.len();
    let mut labels = vec![usize::MAX; n];
    let mut count = 0;
    let mut stack = Vec::new();
    for start in 0..n {
        if labels[start] != usize::MAX {
            continue;
        }
        labels[start] = count;
        stack.push(start);
        while let Some(node) = stack.pop() {
            for &(other, _) in adjacency.neighbours(node) {
                if labels[other] == usize::MAX {
                    labels[other] = count;
                    stack.push(other);
                }
            }
        }
        count += 1;
    }
    (labels, count)
}

fn euclidean(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f64>()
        .sqrt()
}

/// The configured metric between two centroids, in double precision —
/// `pairwise_distances(component_centroids, metric=metric)`.
fn metric_f64(metric: Metric, a: &[f64], b: &[f64]) -> f64 {
    match metric {
        Metric::Euclidean => euclidean(a, b),
        Metric::Manhattan => a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum(),
        Metric::Cosine => {
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let na = a.iter().map(|x| x * x).sum::<f64>().sqrt();
            let nb = b.iter().map(|x| x * x).sum::<f64>().sqrt();
            if na <= 0.0 || nb <= 0.0 {
                1.0
            } else {
                1.0 - dot / (na * nb)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two clean blobs joined by a single weak edge: the first non-trivial
    /// eigenvector (the Fiedler vector) separates them by sign.
    #[test]
    fn the_first_coordinate_separates_two_weakly_joined_groups() {
        let mut edges = Vec::new();
        for group in 0..2 {
            let base = group * 20;
            for i in 0..20 {
                for j in (i + 1)..20 {
                    edges.push((base + i, base + j, 1.0 as Scalar));
                }
            }
        }
        edges.push((0, 20, 0.01));
        let data = vec![0.0 as Scalar; 40];
        let layout = spectral_layout(&data, 40, 1, &edges, 2, Metric::Euclidean, 0);
        let first: Vec<f64> = (0..40).map(|i| layout[i * 2]).collect();
        let left = first[..20].iter().all(|&v| v > 0.0) || first[..20].iter().all(|&v| v < 0.0);
        let right = first[20..].iter().all(|&v| v > 0.0) || first[20..].iter().all(|&v| v < 0.0);
        assert!(left && right, "{first:?}");
        assert!(first[0].signum() != first[20].signum());
    }

    /// A graph in pieces: each piece is laid out around its own centre, and
    /// the pieces do not overlap.
    #[test]
    fn disconnected_pieces_are_placed_apart() {
        let mut edges = Vec::new();
        let mut data = Vec::new();
        for group in 0..3 {
            let base = group * 10;
            for i in 0..10 {
                data.push((group * 100) as Scalar);
                for j in (i + 1)..10 {
                    edges.push((base + i, base + j, 1.0 as Scalar));
                }
            }
        }
        let layout = spectral_layout(&data, 30, 1, &edges, 2, Metric::Euclidean, 0);
        let centre = |g: usize| {
            let mut c = [0.0f64; 2];
            for i in 0..10 {
                c[0] += layout[(g * 10 + i) * 2] / 10.0;
                c[1] += layout[(g * 10 + i) * 2 + 1] / 10.0;
            }
            c
        };
        for a in 0..3 {
            for b in (a + 1)..3 {
                let (ca, cb) = (centre(a), centre(b));
                let gap = ((ca[0] - cb[0]).powi(2) + (ca[1] - cb[1]).powi(2)).sqrt();
                assert!(gap > 0.5, "pieces {a} and {b} overlap: {gap}");
            }
        }
    }

    #[test]
    fn the_layout_depends_on_the_seed_alone() {
        let edges: Vec<(usize, usize, Scalar)> = (0..499)
            .map(|i| (i, i + 1, 1.0))
            .chain([(0, 250, 0.5)])
            .collect();
        let data = vec![0.0 as Scalar; 500];
        let a = spectral_layout(&data, 500, 1, &edges, 2, Metric::Euclidean, 4);
        let b = spectral_layout(&data, 500, 1, &edges, 2, Metric::Euclidean, 4);
        assert_eq!(a, b);
        assert!(a.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn components_are_numbered_in_order_of_appearance() {
        let adjacency = Adjacency::new(5, &[(3, 4, 1.0), (0, 2, 1.0)]);
        let (labels, count) = connected_components(&adjacency);
        assert_eq!(count, 3);
        assert_eq!(labels, vec![0, 1, 0, 2, 2]);
    }
}
