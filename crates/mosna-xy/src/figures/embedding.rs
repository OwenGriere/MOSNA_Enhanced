//! The clustered two-dimensional projection.

use std::path::Path;

use mosna_core::colormap::make_cluster_cmap;

use crate::palette::hex;
use crate::spec::Spec;

pub const KIND: &str = "embedding";

pub fn spec(
    embedding: &[f64],
    n_components: usize,
    labels: &[u32],
    parameters: &str,
    save_dir: &Path,
) -> Option<Spec> {
    if n_components < 2 || labels.is_empty() {
        return None;
    }

    // The first two components are the plane, as the original scattered
    // `embedding[:, 0]` against `embedding[:, 1]`.
    let points: Vec<[f64; 2]> = (0..labels.len())
        .filter_map(|row| {
            let base = row * n_components;
            Some([*embedding.get(base)?, *embedding.get(base + 1)?])
        })
        .collect();
    if points.is_empty() {
        return None;
    }

    let mut clusters: Vec<u32> = labels.to_vec();
    clusters.sort_unstable();
    clusters.dedup();

    let ids: Vec<String> = clusters.iter().map(|id| id.to_string()).collect();
    let colours: Vec<String> = make_cluster_cmap(clusters.len())
        .into_iter()
        .map(hex)
        .collect();

    // The palette is indexed by *rank*, not by identifier: a run that found
    // niches 3 and 7 must not reach past the end of a two-colour palette.
    let ranks: Vec<u32> = labels
        .iter()
        .take(points.len())
        .map(|label| clusters.iter().position(|id| id == label).unwrap_or(0) as u32)
        .collect();

    // Each cluster's identifier is written at its centroid, which is what lets
    // a blob here be matched to a column of the composition heatmap.
    let centroids: Vec<[f64; 2]> = clusters
        .iter()
        .map(|cluster| {
            let members: Vec<&[f64; 2]> = points
                .iter()
                .zip(&ranks)
                .filter(|(_, rank)| {
                    clusters
                        .get(**rank as usize)
                        .map(|id| id == cluster)
                        .unwrap_or(false)
                })
                .map(|(point, _)| point)
                .collect();
            if members.is_empty() {
                return [f64::NAN, f64::NAN];
            }
            let count = members.len() as f64;
            [
                members.iter().map(|p| p[0]).sum::<f64>() / count,
                members.iter().map(|p| p[1]).sum::<f64>() / count,
            ]
        })
        .collect();

    let flat: Vec<f64> = points.iter().flat_map(|p| [p[0], p[1]]).collect();

    Some(
        Spec::new(KIND, format!("cluster_labels{parameters}"), save_dir)
            .set("title", "Spatial niches")
            .set("cluster_ids", serde_json::json!(ids))
            .set("colours", serde_json::json!(colours))
            .set("centroids", serde_json::json!(centroids))
            .set_f64_blob("points", &flat, &[points.len(), 2])
            .set_u32_blob("clusters", &ranks, &[ranks.len()]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A projection with fewer than two dimensions cannot be scattered in a
    /// plane. Nothing is drawn rather than something misleading.
    #[test]
    fn a_projection_that_is_not_a_plane_produces_no_figure() {
        assert!(spec(&[1.0, 2.0], 1, &[0, 1], "", Path::new("/out")).is_none());
        assert!(spec(&[], 2, &[], "", Path::new("/out")).is_none());
    }

    #[test]
    fn the_parameters_are_part_of_the_file_name() {
        let spec = spec(&[0.0, 0.0], 2, &[0], "_metric-cosine", Path::new("/out")).unwrap();
        assert_eq!(spec.stem(), "cluster_labels_metric-cosine");
    }

    #[test]
    fn every_cluster_gets_a_colour_and_a_centroid_to_be_named_at() {
        let embedding = [0.0, 0.0, 2.0, 0.0, 0.0, 4.0];
        let spec = spec(&embedding, 2, &[0, 0, 1], "", Path::new("/out")).unwrap();
        let json = spec.to_json();

        assert_eq!(json["cluster_ids"], serde_json::json!(["0", "1"]));
        assert_eq!(json["colours"][0], hex(make_cluster_cmap(2)[0]));
        // The first cluster holds (0,0) and (2,0); its centroid is between them.
        assert_eq!(json["centroids"][0], serde_json::json!([1.0, 0.0]));
        assert_eq!(json["centroids"][1], serde_json::json!([0.0, 4.0]));
    }

    /// The label array indexes the palette, so it has to be the *rank* of the
    /// cluster among those present — a run that produced niches 3 and 7 must
    /// not index a two-colour palette with a seven.
    #[test]
    fn labels_are_handed_over_as_ranks_not_as_identifiers() {
        let embedding = [0.0, 0.0, 1.0, 1.0];
        let spec = spec(&embedding, 2, &[3, 7], "", Path::new("/out")).unwrap();
        let json = spec.to_json();

        assert_eq!(json["cluster_ids"], serde_json::json!(["3", "7"]));
        assert_eq!(json["clusters"]["shape"], serde_json::json!([2]));
    }

    /// More components than two: the first two are the plane, as the original
    /// scattered `embedding[:, 0]` against `embedding[:, 1]`.
    #[test]
    fn only_the_first_two_components_are_scattered() {
        let embedding = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let spec = spec(&embedding, 3, &[0, 1], "", Path::new("/out")).unwrap();
        assert_eq!(spec.to_json()["points"]["shape"], serde_json::json!([2, 2]));
    }
}
