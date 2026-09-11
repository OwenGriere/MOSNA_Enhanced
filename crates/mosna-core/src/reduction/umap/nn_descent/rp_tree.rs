//! The random projection forest the descent starts from.

use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use crate::reduction::umap::metric::Metric;
use crate::reduction::umap::nn_descent::rng::{wrap_index, TauRand};
use crate::reduction::umap::Scalar;

/// Below this the margin counts as zero and the side is decided by a coin
/// flip. pynndescent's `EPS`.
const EPS: f64 = 1e-8;

/// Recursion limit, as `max_rptree_depth` defaults.
const MAX_DEPTH: usize = 200;

/// The leaves of `n_trees` random projection trees.
///
/// # What the forest is for, and what is left out
///
/// A random projection tree cuts the cloud in half by a hyperplane drawn
/// between two random points, recursively, until each part holds at most
/// `leaf_size` points. Points that keep landing together are plausibly near
/// each other, so joining every pair within a leaf gives the descent a starting
/// graph far better than random — which matters, because descent only ever
/// improves on what it starts with.
///
/// pynndescent keeps the hyperplanes as well, to route a query down the tree
/// when the index is later searched. Nothing here searches: the reduction wants
/// the neighbour graph of the points it was given and never queries the index
/// afterwards. So only the leaves are built, and the hyperplanes are discarded
/// as soon as they have done their cutting.
///
/// Trees are independent and are built in parallel, each from its own stream,
/// drawn up front so the forest does not depend on the order they finish in.
pub fn leaf_array(
    data: &[Scalar],
    n_rows: usize,
    n_features: usize,
    leaf_size: usize,
    n_trees: usize,
    metric: Metric,
    source: &mut ChaCha8Rng,
) -> Vec<Vec<u32>> {
    // Cosine cuts by direction, and a hyperplane through the origin is the cut
    // that respects it; the other two metrics cut by position. This mirrors
    // `NNDescent._angular_trees`.
    let angular = metric == Metric::Cosine;

    let streams: Vec<TauRand> = (0..n_trees).map(|_| TauRand::draw(source)).collect();

    streams
        .into_par_iter()
        .flat_map(|mut rng| {
            let mut leaves = Vec::new();
            let all: Vec<u32> = (0..n_rows as u32).collect();
            split(
                data,
                n_features,
                all,
                leaf_size,
                MAX_DEPTH,
                angular,
                &mut rng,
                &mut leaves,
            );
            leaves
        })
        .collect()
}

/// Cut `indices` in two until the parts are small enough, collecting the parts.
///
/// The left subtree is emitted before the right one, so the leaves come out in
/// the order pynndescent stacks them — which is the order their pairs are
/// offered to the heaps, and therefore part of the result.
#[allow(clippy::too_many_arguments)]
fn split(
    data: &[Scalar],
    n_features: usize,
    indices: Vec<u32>,
    leaf_size: usize,
    depth: usize,
    angular: bool,
    rng: &mut TauRand,
    leaves: &mut Vec<Vec<u32>>,
) {
    if indices.len() <= leaf_size || depth == 0 {
        leaves.push(indices);
        return;
    }

    let (left, right) = if angular {
        angular_split(data, n_features, &indices, rng)
    } else {
        euclidean_split(data, n_features, &indices, rng)
    };

    split(
        data,
        n_features,
        left,
        leaf_size,
        depth - 1,
        angular,
        rng,
        leaves,
    );
    split(
        data,
        n_features,
        right,
        leaf_size,
        depth - 1,
        angular,
        rng,
        leaves,
    );
}

/// Split by a hyperplane halfway between two random points.
///
/// The arithmetic is reproduced as numba runs it: the hyperplane in single
/// precision, the offset and every margin accumulated in double.
fn euclidean_split(
    data: &[Scalar],
    n_features: usize,
    indices: &[u32],
    rng: &mut TauRand,
) -> (Vec<u32>, Vec<u32>) {
    let (left, right) = two_points(indices, rng);
    let (left, right) = (
        &data[left * n_features..(left + 1) * n_features],
        &data[right * n_features..(right + 1) * n_features],
    );

    let mut hyperplane = vec![0 as Scalar; n_features];
    let mut offset = 0.0f64;
    for d in 0..n_features {
        hyperplane[d] = left[d] - right[d];
        offset -= (hyperplane[d] * (left[d] + right[d])) as f64 / 2.0;
    }

    partition(data, n_features, indices, &hyperplane, offset, rng)
}

/// Split by a hyperplane through the origin, between two random directions.
fn angular_split(
    data: &[Scalar],
    n_features: usize,
    indices: &[u32],
    rng: &mut TauRand,
) -> (Vec<u32>, Vec<u32>) {
    let (left, right) = two_points(indices, rng);
    let (left, right) = (
        &data[left * n_features..(left + 1) * n_features],
        &data[right * n_features..(right + 1) * n_features],
    );

    // A point at the origin has no direction; it is given unit length so the
    // division stays finite, as pynndescent does.
    let left_norm = unit_if_degenerate(norm(left));
    let right_norm = unit_if_degenerate(norm(right));

    let mut hyperplane = vec![0 as Scalar; n_features];
    for d in 0..n_features {
        hyperplane[d] = left[d] / left_norm - right[d] / right_norm;
    }

    let hyperplane_norm = unit_if_degenerate(norm(&hyperplane));
    for value in &mut hyperplane {
        *value /= hyperplane_norm;
    }

    partition(data, n_features, indices, &hyperplane, 0.0, rng)
}

/// The two points a hyperplane is drawn between.
///
/// The second draw is nudged past the first when they collide, so a split is
/// never attempted between a point and itself.
fn two_points(indices: &[u32], rng: &mut TauRand) -> (usize, usize) {
    let len = indices.len();
    let left = rng.next_int() % len as i32;
    let mut right = rng.next_int() % len as i32;
    if left == right {
        right += 1;
    }
    right %= len as i32;
    (
        indices[wrap_index(left, len)] as usize,
        indices[wrap_index(right, len)] as usize,
    )
}

/// Send each point to the side of the hyperplane its margin puts it on.
///
/// A point on the hyperplane goes to a side decided by a coin flip, and a split
/// that leaves one side empty — which happens when the cloud is degenerate, not
/// when the hyperplane is bad — is replaced outright by a random one.
fn partition(
    data: &[Scalar],
    n_features: usize,
    indices: &[u32],
    hyperplane: &[Scalar],
    offset: f64,
    rng: &mut TauRand,
) -> (Vec<u32>, Vec<u32>) {
    let mut side = Vec::with_capacity(indices.len());
    let mut n_left = 0usize;

    for &index in indices {
        let row = &data[index as usize * n_features..(index as usize + 1) * n_features];
        let mut margin = offset;
        for d in 0..n_features {
            margin += (hyperplane[d] * row[d]) as f64;
        }

        let chosen = if margin.abs() < EPS {
            rng.next_side()
        } else if margin > 0.0 {
            0
        } else {
            1
        };
        n_left += usize::from(chosen == 0);
        side.push(chosen);
    }

    if n_left == 0 || n_left == indices.len() {
        n_left = 0;
        for slot in &mut side {
            *slot = rng.next_side();
            n_left += usize::from(*slot == 0);
        }
    }

    let mut left = Vec::with_capacity(n_left);
    let mut right = Vec::with_capacity(indices.len() - n_left);
    for (&index, &chosen) in indices.iter().zip(&side) {
        if chosen == 0 {
            left.push(index);
        } else {
            right.push(index);
        }
    }
    (left, right)
}

/// pynndescent's `norm`: accumulated in double, returned in single.
fn norm(vector: &[Scalar]) -> Scalar {
    let mut total = 0.0f64;
    for value in vector {
        total += (value * value) as f64;
    }
    total.sqrt() as Scalar
}

/// Guard a division by a norm that has collapsed.
fn unit_if_degenerate(value: Scalar) -> Scalar {
    if (value as f64).abs() < EPS {
        1.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn grid(side: usize) -> (Vec<Scalar>, usize) {
        let mut data = Vec::new();
        for x in 0..side {
            for y in 0..side {
                data.push(x as Scalar);
                data.push(y as Scalar);
            }
        }
        (data, side * side)
    }

    #[test]
    fn every_point_lands_in_exactly_one_leaf_of_each_tree() {
        let (data, n_rows) = grid(20);
        let mut source = ChaCha8Rng::seed_from_u64(1);
        let leaves = leaf_array(&data, n_rows, 2, 10, 3, Metric::Euclidean, &mut source);

        let mut seen = vec![0usize; n_rows];
        for leaf in &leaves {
            for &point in leaf {
                seen[point as usize] += 1;
            }
        }
        assert!(seen.iter().all(|&count| count == 3), "{seen:?}");
    }

    #[test]
    fn leaves_are_no_larger_than_asked() {
        let (data, n_rows) = grid(24);
        let mut source = ChaCha8Rng::seed_from_u64(2);
        let leaves = leaf_array(&data, n_rows, 2, 16, 2, Metric::Euclidean, &mut source);
        assert!(leaves.iter().all(|leaf| leaf.len() <= 16));
        assert!(leaves.len() >= 2 * (n_rows / 16));
    }

    #[test]
    fn the_forest_is_reproducible() {
        let (data, n_rows) = grid(16);
        let mut first = ChaCha8Rng::seed_from_u64(5);
        let mut second = ChaCha8Rng::seed_from_u64(5);
        let mut other = ChaCha8Rng::seed_from_u64(6);
        assert_eq!(
            leaf_array(&data, n_rows, 2, 12, 3, Metric::Euclidean, &mut first),
            leaf_array(&data, n_rows, 2, 12, 3, Metric::Euclidean, &mut second)
        );
        assert_ne!(
            leaf_array(&data, n_rows, 2, 12, 3, Metric::Euclidean, &mut first),
            leaf_array(&data, n_rows, 2, 12, 3, Metric::Euclidean, &mut other)
        );
    }

    /// Points that are all identical cannot be separated by any hyperplane;
    /// the random fallback must still split them rather than recurse forever.
    #[test]
    fn a_degenerate_cloud_still_terminates() {
        let n_rows = 200;
        let data = vec![1.0 as Scalar; n_rows * 2];
        let mut source = ChaCha8Rng::seed_from_u64(3);
        let leaves = leaf_array(&data, n_rows, 2, 10, 1, Metric::Euclidean, &mut source);
        assert!(leaves.iter().all(|leaf| leaf.len() <= 10));
        assert_eq!(leaves.iter().map(Vec::len).sum::<usize>(), n_rows);
    }

    /// Angular trees cut by direction, so how far a point sits from the origin
    /// must not decide which leaf it lands in.
    ///
    /// Stretching every point along its own ray leaves the normalised
    /// directions — and therefore every hyperplane and the sign of every
    /// margin — untouched. Euclidean trees are shown alongside to make the
    /// point that this is a property of the angular split and not of the test.
    #[test]
    fn angular_leaves_are_blind_to_scale() {
        use rand::Rng;

        let (n_rows, n_features) = (300, 4);
        let mut source = ChaCha8Rng::seed_from_u64(9);
        let data: Vec<Scalar> = (0..n_rows * n_features)
            .map(|_| source.gen_range(0.1..10.0) as Scalar)
            .collect();

        let mut stretched = data.clone();
        for row in 0..n_rows {
            let scale = source.gen_range(0.2..20.0) as Scalar;
            for value in &mut stretched[row * n_features..(row + 1) * n_features] {
                *value *= scale;
            }
        }

        let leaves = |data: &[Scalar], metric| {
            let mut source = ChaCha8Rng::seed_from_u64(4);
            leaf_array(data, n_rows, n_features, 30, 2, metric, &mut source)
        };

        assert_eq!(
            leaves(&data, Metric::Cosine),
            leaves(&stretched, Metric::Cosine)
        );
        assert_ne!(
            leaves(&data, Metric::Euclidean),
            leaves(&stretched, Metric::Euclidean)
        );
    }
}
