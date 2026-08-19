//! One sample's spatial network.

use std::path::Path;

use mosna_core::colormap::make_cluster_cmap;
use mosna_io::SampleId;

use crate::palette::hex;
use crate::spec::Spec;

pub const KIND: &str = "network";

/// The file the interface's gallery groups by parsing, so the name is a
/// contract: `net_{patient}.png` or `net_{patient}-{sample}.png`.
pub fn stem(sample: &SampleId, sample_column: Option<&str>) -> String {
    match (&sample.sample, sample_column) {
        (Some(id), Some(_)) => format!("net_{}-{}", sample.patient, id),
        _ => format!("net_{}", sample.patient),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spec(
    sample: &SampleId,
    patient_column: &str,
    sample_column: Option<&str>,
    coords: &[[f64; 2]],
    pairs: &[(u32, u32)],
    labels: &[String],
    save_dir: &Path,
) -> Spec {
    let title = match (&sample.sample, sample_column) {
        (Some(id), Some(column)) => format!(
            "Tysserand network {patient_column} {} and {column} {id}",
            sample.patient
        ),
        _ => format!("Tysserand network {patient_column} {}", sample.patient),
    };

    // First-seen order, so a sample's colours do not depend on how its cells
    // happen to be sorted in the file it was read from.
    let mut vocabulary: Vec<&str> = Vec::new();
    for label in labels {
        if !vocabulary.contains(&label.as_str()) {
            vocabulary.push(label);
        }
    }
    let colours: Vec<String> = make_cluster_cmap(vocabulary.len())
        .into_iter()
        .map(hex)
        .collect();

    let index: Vec<u32> = labels
        .iter()
        .map(|label| {
            vocabulary
                .iter()
                .position(|known| *known == label.as_str())
                .unwrap_or(0) as u32
        })
        .collect();

    let points: Vec<f64> = coords
        .iter()
        .flat_map(|point| [point[0], point[1]])
        .collect();
    let edges: Vec<u32> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();

    Spec::new(KIND, stem(sample, sample_column), save_dir)
        .set("title", title)
        .set("legend_title", "Phenotype")
        .set("phenotypes", serde_json::json!(vocabulary))
        .set("colours", serde_json::json!(colours))
        .set_f64_blob("coords", &points, &[coords.len(), 2])
        .set_u32_blob("edges", &edges, &[pairs.len(), 2])
        .set_u32_blob("phenotype_index", &index, &[labels.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SampleId {
        SampleId {
            patient: "4".to_string(),
            sample: Some("9".to_string()),
        }
    }

    #[test]
    fn the_file_name_is_the_one_the_gallery_parses() {
        assert_eq!(stem(&sample(), Some("chunk")), "net_4-9");
        assert_eq!(stem(&SampleId::patient_only("1"), None), "net_1");
        assert_eq!(
            stem(&sample(), None),
            "net_4",
            "without a sample column the sample is not part of the name"
        );
    }

    #[test]
    fn the_title_spells_out_the_columns_it_grouped_by() {
        let spec = spec(
            &sample(),
            "patient",
            Some("chunk"),
            &[[0.0, 0.0]],
            &[],
            &["A".to_string()],
            Path::new("/out"),
        );
        assert_eq!(
            spec.to_json()["title"],
            "Tysserand network patient 4 and chunk 9"
        );
    }

    /// The vocabulary is in first-seen order, so a sample's colours do not
    /// depend on how its cells happen to be sorted in the file.
    #[test]
    fn phenotypes_keep_the_order_they_first_appeared_in() {
        let labels = vec!["B".to_string(), "A".to_string(), "B".to_string()];
        let coords = [[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]];
        let spec = spec(
            &sample(),
            "p",
            None,
            &coords,
            &[],
            &labels,
            Path::new("/out"),
        );
        let json = spec.to_json();

        assert_eq!(json["phenotypes"], serde_json::json!(["B", "A"]));
        assert_eq!(json["colours"][0], hex(make_cluster_cmap(2)[0]));
    }

    #[test]
    fn the_cells_and_the_edges_are_handed_over_as_arrays() {
        let coords = [[0.0, 1.0], [2.0, 3.0]];
        let spec = spec(
            &sample(),
            "p",
            None,
            &coords,
            &[(0, 1)],
            &["A".to_string(), "A".to_string()],
            Path::new("/out"),
        );
        let json = spec.to_json();

        assert_eq!(json["coords"]["shape"], serde_json::json!([2, 2]));
        assert_eq!(json["edges"]["shape"], serde_json::json!([1, 2]));
        assert_eq!(json["phenotype_index"]["shape"], serde_json::json!([2]));
    }

    /// A sample whose file was empty still produces a specification: the
    /// renderer decides there is nothing to draw, in one place, rather than
    /// each analysis deciding it differently.
    #[test]
    fn a_sample_with_no_cells_is_still_a_specification() {
        let spec = spec(&sample(), "p", None, &[], &[], &[], Path::new("/out"));
        assert_eq!(spec.to_json()["coords"]["shape"], serde_json::json!([0, 2]));
    }
}
