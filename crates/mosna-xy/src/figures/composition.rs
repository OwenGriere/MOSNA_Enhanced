//! What each niche is made of.

use std::path::Path;

use mosna_core::colormap::blues;
use mosna_core::niches::{NicheComposition, Normalize};

use crate::palette;
use crate::spec::Spec;

pub const KIND: &str = "niche_composition";

/// The file name carries the normalisation, because `normalize: all` produces
/// one figure per variant into the same directory.
pub fn stem(normalize: Normalize) -> String {
    format!("Niches_Aggregated_Composition_{}", normalize.as_str())
}

pub fn spec(composition: &NicheComposition, normalize: Normalize, save_dir: &Path) -> Spec {
    let rows = composition.phenotypes.len();
    let columns = composition.niches.len();

    // Scaled to the data, as seaborn does by default: the map has to spend its
    // whole range on the values that are actually there.
    let (low, high) = composition
        .counts
        .iter()
        .filter(|value| value.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), value| {
            (lo.min(*value), hi.max(*value))
        });
    let (low, high) = if low.is_finite() && high > low {
        (low, high)
    } else if low.is_finite() {
        // Every cell identical: one flat colour is the honest picture, and a
        // zero-width domain would leave the renderer dividing by zero.
        (low, low + 1.0)
    } else {
        (0.0, 1.0)
    };

    let niches: Vec<String> = composition.niches.iter().map(|id| id.to_string()).collect();

    // The figure grows with the vocabulary rather than squeezing forty
    // phenotypes into eight inches, as `max(8, n * 0.35)` did.
    let height = ((rows as f64 * 0.35).max(8.0) * 100.0).round() as i64;

    Spec::new(KIND, stem(normalize), save_dir)
        .set("title", "Niches Aggregated Composition")
        .set("colorbar_title", normalize.as_str())
        .set("y_labels", serde_json::json!(composition.phenotypes))
        .set("x_labels", serde_json::json!(niches))
        .set(
            "colormap",
            serde_json::json!(palette::linear(&blues(), palette::STOPS)),
        )
        .set("domain", serde_json::json!([low, high]))
        .set("width", 2000)
        .set("height", height)
        .set_f64_blob("z", &composition.counts, &[rows, columns])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composition() -> NicheComposition {
        NicheComposition {
            phenotypes: vec!["A".to_string(), "B".to_string()],
            niches: vec![0, 1, 2],
            counts: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        }
    }

    #[test]
    fn the_normalisation_is_part_of_the_file_name() {
        assert_eq!(
            stem(Normalize::Total),
            "Niches_Aggregated_Composition_total"
        );
        assert_eq!(
            stem(Normalize::NicheAndObs),
            "Niches_Aggregated_Composition_niche&obs"
        );
    }

    #[test]
    fn the_matrix_is_phenotypes_down_and_niches_across() {
        let json = spec(&composition(), Normalize::Total, Path::new("/out")).to_json();

        assert_eq!(json["z"]["shape"], serde_json::json!([2, 3]));
        assert_eq!(json["y_labels"], serde_json::json!(["A", "B"]));
        assert_eq!(json["x_labels"], serde_json::json!(["0", "1", "2"]));
    }

    /// Scaled to the data, as seaborn does by default: the map has to spend
    /// its whole range on the values that are actually there.
    #[test]
    fn the_scale_runs_between_the_smallest_and_the_largest_value() {
        let json = spec(&composition(), Normalize::Total, Path::new("/out")).to_json();
        assert_eq!(json["domain"], serde_json::json!([1.0, 6.0]));
        assert_eq!(json["colormap"][0], palette::hex(blues().sample(0.0)));
    }

    /// Every cell identical: a zero-width domain would leave the renderer
    /// dividing by zero, and one flat colour is the honest picture anyway.
    #[test]
    fn a_composition_with_no_variation_still_has_a_usable_domain() {
        let flat = NicheComposition {
            phenotypes: vec!["A".to_string()],
            niches: vec![0, 1],
            counts: vec![2.0, 2.0],
        };
        let json = spec(&flat, Normalize::Total, Path::new("/out")).to_json();
        let domain = &json["domain"];
        assert!(domain[0].as_f64().unwrap() < domain[1].as_f64().unwrap());
    }

    #[test]
    fn an_empty_composition_is_still_a_specification() {
        let empty = NicheComposition {
            phenotypes: Vec::new(),
            niches: Vec::new(),
            counts: Vec::new(),
        };
        let json = spec(&empty, Normalize::Total, Path::new("/out")).to_json();
        assert_eq!(json["z"]["shape"], serde_json::json!([0, 0]));
    }
}
