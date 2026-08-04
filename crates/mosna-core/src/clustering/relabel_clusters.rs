//! Port of `clustering.py::relabel_clusters`.

/// Renumber cluster ids to the contiguous range `0..k`.
///
/// ```python
/// bins, counts = np.unique(clusters, return_counts=True)
/// if len(bins) == bins.max() - 1:
///     return clusters
/// return np.searchsorted(bins, clusters).astype(clusters.dtype)
/// ```
///
/// The early-return condition in the Python is wrong — for an already-contiguous
/// `0..k` labelling, `len(bins)` is `k + 1` while `bins.max() - 1` is `k - 1`,
/// so it never fires and the `searchsorted` always runs. That is harmless,
/// because `searchsorted` is the identity on a contiguous labelling, and this
/// port simply always does the mapping.
///
/// Contiguity matters downstream: niche labels index colour maps and the
/// columns of the composition matrix, so a gap would either mis-colour a figure
/// or index out of bounds.
pub fn relabel_clusters(clusters: &[u32]) -> Vec<u32> {
    if clusters.is_empty() {
        return Vec::new();
    }

    let mut distinct: Vec<u32> = clusters.to_vec();
    distinct.sort_unstable();
    distinct.dedup();

    // `searchsorted` maps each label to its rank among the sorted distinct
    // values, which is exactly the contiguous renumbering.
    clusters
        .iter()
        .map(|label| {
            distinct
                .binary_search(label)
                .expect("every label is in the distinct set") as u32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_gaps_in_the_numbering() {
        assert_eq!(relabel_clusters(&[5, 9, 5, 20, 9]), vec![0, 1, 0, 2, 1]);
    }

    #[test]
    fn is_the_identity_on_a_contiguous_labelling() {
        let labels = vec![0, 1, 2, 1, 0];
        assert_eq!(relabel_clusters(&labels), labels);
    }

    #[test]
    fn preserves_which_points_share_a_cluster() {
        let labels = [7, 7, 3, 3, 11];
        let relabelled = relabel_clusters(&labels);
        assert_eq!(relabelled[0], relabelled[1]);
        assert_eq!(relabelled[2], relabelled[3]);
        assert_ne!(relabelled[0], relabelled[2]);
        assert_ne!(relabelled[4], relabelled[0]);
    }

    #[test]
    fn the_result_starts_at_zero_and_has_no_gaps() {
        let relabelled = relabel_clusters(&[100, 4, 62, 4]);
        let mut distinct = relabelled.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct, vec![0, 1, 2]);
    }

    #[test]
    fn handles_degenerate_inputs() {
        assert!(relabel_clusters(&[]).is_empty());
        assert_eq!(relabel_clusters(&[7]), vec![0]);
        assert_eq!(relabel_clusters(&[3, 3, 3]), vec![0, 0, 0]);
    }
}
