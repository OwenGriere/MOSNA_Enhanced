//! K-means with k-means++ seeding.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Cluster assignment and centroids.
#[derive(Debug, Clone)]
pub struct KMeansResult {
    pub labels: Vec<u32>,
    /// Row-major `k * n_features`.
    pub centroids: Vec<f64>,
    pub k: usize,
    pub n_features: usize,
    /// Sum of squared distances to the assigned centroid.
    pub inertia: f64,
}

/// Partition `data` (row-major, `n_rows * n_features`) into `k` clusters.
///
/// Seeded with k-means++ and run to convergence or `max_iter`, restarting
/// `n_init` times and keeping the best inertia — the same strategy
/// `sklearn.cluster.KMeans` uses by default. The generator is seeded from
/// `seed`, so a run is reproducible.
///
/// This backs the final step of spectral clustering and the initialisation of
/// the Gaussian mixture.
pub fn kmeans(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    max_iter: usize,
    n_init: usize,
    seed: u64,
) -> KMeansResult {
    let k = k.max(1).min(n_rows.max(1));

    let mut best: Option<KMeansResult> = None;
    for attempt in 0..n_init.max(1) {
        let candidate = run_once(
            data,
            n_rows,
            n_features,
            k,
            max_iter,
            seed.wrapping_add(attempt as u64),
        );
        if best.as_ref().is_none_or(|b| candidate.inertia < b.inertia) {
            best = Some(candidate);
        }
    }
    best.expect("n_init is at least one")
}

fn run_once(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    max_iter: usize,
    seed: u64,
) -> KMeansResult {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut centroids = kmeans_plus_plus(data, n_rows, n_features, k, &mut rng);
    let mut labels = vec![0u32; n_rows];
    let mut inertia = f64::INFINITY;

    for _ in 0..max_iter.max(1) {
        // Assignment step.
        let mut changed = false;
        let mut new_inertia = 0.0;
        for row in 0..n_rows {
            let point = &data[row * n_features..(row + 1) * n_features];
            let (best, best_dist) = nearest(point, &centroids, k, n_features);
            new_inertia += best_dist;
            if labels[row] != best {
                labels[row] = best;
                changed = true;
            }
        }

        // Update step.
        let mut sums = vec![0.0f64; k * n_features];
        let mut counts = vec![0usize; k];
        for row in 0..n_rows {
            let cluster = labels[row] as usize;
            counts[cluster] += 1;
            for f in 0..n_features {
                sums[cluster * n_features + f] += data[row * n_features + f];
            }
        }
        for cluster in 0..k {
            if counts[cluster] == 0 {
                // An emptied cluster is re-seeded on the point furthest from
                // its centroid, which is how sklearn avoids losing a cluster.
                if let Some(far) = furthest_point(data, n_rows, n_features, &centroids, k) {
                    centroids[cluster * n_features..(cluster + 1) * n_features]
                        .copy_from_slice(&data[far * n_features..(far + 1) * n_features]);
                }
                continue;
            }
            let denominator = counts[cluster] as f64;
            for f in 0..n_features {
                centroids[cluster * n_features + f] = sums[cluster * n_features + f] / denominator;
            }
        }

        inertia = new_inertia;
        if !changed {
            break;
        }
    }

    KMeansResult {
        labels,
        centroids,
        k,
        n_features,
        inertia,
    }
}

/// k-means++ seeding: each new centre is drawn with probability proportional to
/// its squared distance to the nearest existing centre.
fn kmeans_plus_plus(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    k: usize,
    rng: &mut ChaCha8Rng,
) -> Vec<f64> {
    let mut centroids = Vec::with_capacity(k * n_features);
    if n_rows == 0 {
        return vec![0.0; k * n_features];
    }

    let first = rng.gen_range(0..n_rows);
    centroids.extend_from_slice(&data[first * n_features..(first + 1) * n_features]);

    let mut closest: Vec<f64> = (0..n_rows)
        .map(|row| {
            squared_distance(
                &data[row * n_features..(row + 1) * n_features],
                &centroids[..n_features],
            )
        })
        .collect();

    for _ in 1..k {
        let total: f64 = closest.iter().sum();
        let chosen = if total <= 0.0 || !total.is_finite() {
            // Every point coincides with a centre; any index will do.
            rng.gen_range(0..n_rows)
        } else {
            let target = rng.gen_range(0.0..total);
            let mut running = 0.0;
            let mut pick = n_rows - 1;
            for (row, &weight) in closest.iter().enumerate() {
                running += weight;
                if running >= target {
                    pick = row;
                    break;
                }
            }
            pick
        };

        let new_centre = &data[chosen * n_features..(chosen + 1) * n_features];
        centroids.extend_from_slice(new_centre);

        for row in 0..n_rows {
            let d = squared_distance(&data[row * n_features..(row + 1) * n_features], new_centre);
            if d < closest[row] {
                closest[row] = d;
            }
        }
    }
    centroids
}

fn nearest(point: &[f64], centroids: &[f64], k: usize, n_features: usize) -> (u32, f64) {
    let mut best = 0u32;
    let mut best_dist = f64::INFINITY;
    for cluster in 0..k {
        let centre = &centroids[cluster * n_features..(cluster + 1) * n_features];
        let d = squared_distance(point, centre);
        if d < best_dist {
            best_dist = d;
            best = cluster as u32;
        }
    }
    (best, best_dist)
}

fn furthest_point(
    data: &[f64],
    n_rows: usize,
    n_features: usize,
    centroids: &[f64],
    k: usize,
) -> Option<usize> {
    (0..n_rows)
        .map(|row| {
            let point = &data[row * n_features..(row + 1) * n_features];
            (row, nearest(point, centroids, k, n_features).1)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(row, _)| row)
}

fn squared_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three well-separated blobs in the plane.
    fn blobs() -> (Vec<f64>, usize) {
        let mut data = Vec::new();
        for (cx, cy) in [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)] {
            for i in 0..20 {
                let t = i as f64 * 0.1;
                data.push(cx + t.sin() * 0.3);
                data.push(cy + t.cos() * 0.3);
            }
        }
        (data, 60)
    }

    #[test]
    fn separates_well_separated_blobs() {
        let (data, n) = blobs();
        let result = kmeans(&data, n, 2, 3, 100, 5, 0);
        assert_eq!(result.labels.len(), n);

        // Each block of 20 points must share one label.
        for block in 0..3 {
            let first = result.labels[block * 20];
            assert!(
                result.labels[block * 20..(block + 1) * 20]
                    .iter()
                    .all(|&l| l == first),
                "blob {block} was split"
            );
        }
        // And the three labels must be distinct.
        let mut used: Vec<u32> = (0..3).map(|b| result.labels[b * 20]).collect();
        used.sort_unstable();
        used.dedup();
        assert_eq!(used.len(), 3);
    }

    #[test]
    fn is_reproducible_for_a_fixed_seed() {
        let (data, n) = blobs();
        let a = kmeans(&data, n, 2, 3, 100, 3, 42);
        let b = kmeans(&data, n, 2, 3, 100, 3, 42);
        assert_eq!(a.labels, b.labels);
        assert_eq!(a.centroids, b.centroids);
    }

    #[test]
    fn centroids_sit_at_the_blob_centres() {
        let (data, n) = blobs();
        let result = kmeans(&data, n, 2, 3, 100, 5, 1);
        let mut centres: Vec<(f64, f64)> = (0..3)
            .map(|c| (result.centroids[c * 2], result.centroids[c * 2 + 1]))
            .collect();
        centres.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let expected = [(0.0, 0.0), (0.0, 10.0), (10.0, 0.0)];
        for (got, want) in centres.iter().zip(expected) {
            assert!(
                (got.0 - want.0).abs() < 0.5 && (got.1 - want.1).abs() < 0.5,
                "centroid {got:?} is not near {want:?}"
            );
        }
    }

    #[test]
    fn k_is_clamped_to_the_number_of_points() {
        let data = vec![0.0, 0.0, 1.0, 1.0];
        let result = kmeans(&data, 2, 2, 10, 50, 1, 0);
        assert_eq!(result.k, 2);
        assert_eq!(result.labels.len(), 2);
    }

    #[test]
    fn identical_points_do_not_break_seeding() {
        let data = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let result = kmeans(&data, 3, 2, 2, 50, 2, 0);
        assert_eq!(result.labels.len(), 3);
        assert!(result.inertia.abs() < 1e-12);
    }
}
