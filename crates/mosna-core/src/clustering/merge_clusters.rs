//! Port of `clustering.py::{merge_clusters, merge_clusters_until}`.

use crate::clustering::relabel_clusters::relabel_clusters;
use crate::stats::percentile::percentile;

/// Merge the smallest cluster into whichever cluster is closest to it.
///
/// The smallest cluster is absorbed when it holds fewer points than
/// `percentile(sizes, size_perc) * ratio_size`, or unconditionally when
/// `force_merge` is set. The destination is decided by a majority vote among
/// the nearest out-of-cluster neighbours of its `n_neigh_max` most peripheral
/// points — a single nearest neighbour would let one outlier decide.
///
/// `coords` is row-major, one row per label.
///
/// Returns the labels and whether anything was merged.
#[allow(clippy::too_many_arguments)]
pub fn merge_clusters(
    clusters: &[u32],
    coords: &[f64],
    n_features: usize,
    size_thresh: Option<f64>,
    size_perc: f64,
    ratio_size: f64,
    n_neigh_max: usize,
    force_merge: bool,
) -> (Vec<u32>, bool) {
    let mut sizes: std::collections::BTreeMap<u32, usize> = Default::default();
    for &label in clusters {
        *sizes.entry(label).or_insert(0) += 1;
    }
    if sizes.len() <= 1 {
        return (clusters.to_vec(), false);
    }

    let (&smallest_id, &smallest_size) = sizes
        .iter()
        .min_by_key(|&(id, size)| (*size, *id))
        .expect("at least two clusters");

    let threshold = size_thresh.unwrap_or_else(|| {
        let counts: Vec<f64> = sizes.values().map(|&s| s as f64).collect();
        percentile(&counts, size_perc).unwrap_or(0.0) * ratio_size
    });

    if !force_merge && (smallest_size as f64) >= threshold {
        return (clusters.to_vec(), false);
    }

    // Points inside the cluster to dissolve, and everything else.
    let inside: Vec<usize> = clusters
        .iter()
        .enumerate()
        .filter(|&(_, &l)| l == smallest_id)
        .map(|(i, _)| i)
        .collect();
    let outside: Vec<usize> = clusters
        .iter()
        .enumerate()
        .filter(|&(_, &l)| l != smallest_id)
        .map(|(i, _)| i)
        .collect();

    if outside.is_empty() {
        return (clusters.to_vec(), false);
    }

    // Nearest outside point for each inside point, and how far it is.
    let mut nearest: Vec<(f64, u32)> = inside
        .iter()
        .map(|&i| {
            let point = &coords[i * n_features..(i + 1) * n_features];
            outside
                .iter()
                .map(|&j| {
                    let other = &coords[j * n_features..(j + 1) * n_features];
                    let d: f64 = point
                        .iter()
                        .zip(other)
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum();
                    (d, clusters[j])
                })
                .min_by(|a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.1.cmp(&b.1))
                })
                .expect("`outside` is not empty")
        })
        .collect();

    // Keep the closest few: those are the points genuinely touching another
    // cluster, rather than the ones deep inside the doomed one.
    nearest.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    nearest.truncate(n_neigh_max.min(smallest_size).max(1));

    let mut votes: std::collections::BTreeMap<u32, usize> = Default::default();
    for &(_, label) in &nearest {
        *votes.entry(label).or_insert(0) += 1;
    }
    let destination = votes
        .into_iter()
        .max_by_key(|&(label, count)| (count, std::cmp::Reverse(label)))
        .map(|(label, _)| label)
        .expect("at least one vote");

    let merged = clusters
        .iter()
        .map(|&l| if l == smallest_id { destination } else { l })
        .collect();
    (merged, true)
}

/// Merge repeatedly until nothing more can be merged, or a target count is hit.
///
/// Port of `merge_clusters_until`. With `force_n_clust` the loop keeps merging
/// past the size threshold until `cond_n_clust` is reached.
#[allow(clippy::too_many_arguments)]
pub fn merge_clusters_until(
    clusters: &[u32],
    coords: &[f64],
    n_features: usize,
    cond_n_clust: Option<usize>,
    force_n_clust: bool,
    size_thresh: Option<f64>,
    size_perc: f64,
    ratio_size: f64,
    n_neigh_max: usize,
) -> Vec<u32> {
    let mut current = clusters.to_vec();

    loop {
        let (next, merged) = merge_clusters(
            &current,
            coords,
            n_features,
            size_thresh,
            size_perc,
            ratio_size,
            n_neigh_max,
            force_n_clust,
        );
        current = next;

        if !merged && !force_n_clust {
            break;
        }
        if !merged {
            // Forced merging has run out of clusters to combine.
            break;
        }

        let distinct = {
            let mut d = current.clone();
            d.sort_unstable();
            d.dedup();
            d.len()
        };
        if distinct <= 1 {
            break;
        }
        if let Some(target) = cond_n_clust {
            if distinct <= target {
                break;
            }
        }
    }

    relabel_clusters(&current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n_clusters(labels: &[u32]) -> usize {
        let mut d = labels.to_vec();
        d.sort_unstable();
        d.dedup();
        d.len()
    }

    /// Two groups plus a stray point sitting beside the second one.
    fn stray_case() -> (Vec<f64>, Vec<u32>) {
        let coords = vec![
            0.0, 0.0, 0.1, 0.0, 0.2, 0.0, // cluster 0
            10.0, 0.0, 10.1, 0.0, 10.2, 0.0, // cluster 1
            10.3, 0.0, // the stray, cluster 2
        ];
        (coords, vec![0, 0, 0, 1, 1, 1, 2])
    }

    #[test]
    fn the_stray_joins_its_nearest_cluster() {
        let (coords, labels) = stray_case();
        let (merged, did) = merge_clusters(&labels, &coords, 2, None, 25.0, 0.1, 10, true);
        assert!(did);
        assert_eq!(merged[6], merged[3]);
        assert_eq!(n_clusters(&merged), 2);
    }

    #[test]
    fn balanced_clusters_are_left_alone() {
        let coords = vec![0.0, 0.0, 0.1, 0.0, 10.0, 0.0, 10.1, 0.0];
        let labels = vec![0, 0, 1, 1];
        let (unchanged, did) = merge_clusters(&labels, &coords, 2, None, 25.0, 0.1, 10, false);
        assert!(!did);
        assert_eq!(unchanged, labels);
    }

    #[test]
    fn an_explicit_threshold_overrides_the_percentile_rule() {
        let (coords, labels) = stray_case();
        // A threshold of 2 makes the one-point cluster too small.
        let (merged, did) = merge_clusters(&labels, &coords, 2, Some(2.0), 25.0, 0.1, 10, false);
        assert!(did);
        assert_eq!(n_clusters(&merged), 2);
    }

    #[test]
    fn a_single_cluster_cannot_be_merged() {
        let (unchanged, did) =
            merge_clusters(&[0, 0], &[0.0, 0.0, 1.0, 1.0], 2, None, 25.0, 0.1, 10, true);
        assert!(!did);
        assert_eq!(unchanged, vec![0, 0]);
    }

    #[test]
    fn merging_until_a_target_stops_there() {
        let coords: Vec<f64> = (0..20).flat_map(|i| [i as f64, 0.0]).collect();
        let labels: Vec<u32> = (0..20).map(|i| i as u32 / 2).collect();

        let merged = merge_clusters_until(&labels, &coords, 2, Some(4), true, None, 25.0, 0.1, 10);
        assert!(n_clusters(&merged) <= 4, "got {}", n_clusters(&merged));
    }

    #[test]
    fn the_result_is_always_relabelled_contiguously() {
        let (coords, labels) = stray_case();
        let merged = merge_clusters_until(&labels, &coords, 2, Some(2), true, None, 25.0, 0.1, 10);
        let mut distinct = merged.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct, (0..distinct.len() as u32).collect::<Vec<_>>());
    }

    #[test]
    fn forced_merging_terminates_at_a_single_cluster() {
        let coords: Vec<f64> = (0..8).flat_map(|i| [i as f64, 0.0]).collect();
        let labels: Vec<u32> = (0..8).map(|i| i as u32).collect();
        let merged = merge_clusters_until(&labels, &coords, 2, None, true, None, 25.0, 0.1, 10);
        assert_eq!(n_clusters(&merged), 1);
    }
}
