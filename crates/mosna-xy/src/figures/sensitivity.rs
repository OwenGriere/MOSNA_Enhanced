//! The three figures a finished sweep is read through.
//!
//! They answer three different questions about the same set of runs, and a
//! sweep is not readable without all three:
//!
//! * [`fn@agreement`] — where along the grid does the answer actually change?
//! * [`fn@matrix`] — which runs agree with which, when there is no order to
//!   walk along?
//! * [`fn@stability`] — which niches survive the sweep, and which dissolve?
//!
//! None of them is the niche count, which is what the comparison table already
//! shows and what cannot answer any of the three: a run can go from eight
//! niches to eight niches while reassigning half the cells.

use std::path::Path;

use crate::spec::Spec;

/// The two series of the agreement figure, in the order they are given.
///
/// Distinguishable to a reader who cannot tell red from green, which rules out
/// the obvious pairing: this is a figure whose whole content is two lines
/// either lying on top of each other or not.
pub const ARI_COLOUR: &str = "#1f4e99";
pub const AMI_COLOUR: &str = "#c47f17";

pub const AGREEMENT_KIND: &str = "sweep_agreement";
pub const AGREEMENT_STEM: &str = "Sensitivity_Agreement";

pub const MATRIX_KIND: &str = "sweep_matrix";
pub const MATRIX_STEM: &str = "Sensitivity_Agreement_Matrix";

pub const STABILITY_KIND: &str = "sweep_stability";
pub const STABILITY_STEM: &str = "Sensitivity_Niche_Stability";

/// Agreement between consecutive runs, as two lines over the grid's order.
///
/// Two measures rather than one because they disagree where the niches are
/// uneven — the pair count is dominated by the large ones, the information
/// measure is not — and a reader who sees them part company has learnt
/// something a single line would have hidden.
pub fn agreement(pairs: &[String], ari: &[f64], ami: &[f64], save_dir: &Path) -> Spec {
    // The two lines are named here rather than in the renderer, as every other
    // figure's colours are: which series is which is a decision, and the
    // renderer is where drawing happens rather than where meaning is chosen.
    let colours = [ARI_COLOUR, AMI_COLOUR];

    Spec::new(AGREEMENT_KIND, AGREEMENT_STEM, save_dir)
        .set("colours", serde_json::json!(colours))
        .set("title", "Agreement between consecutive runs")
        .set("pairs", serde_json::json!(pairs))
        .set_f64_blob("ari", ari, &[ari.len()])
        .set_f64_blob("ami", ami, &[ami.len()])
        .set("width", 1400)
        .set("height", 700)
}

/// Every run against every other, as a heatmap.
///
/// Square and symmetric, so it is read for its blocks: a group of runs that
/// agree with each other and not with the rest is a family of settings telling
/// one story, and that is the reading a sweep over a categorical parameter
/// needs.
pub fn matrix(runs: &[String], values: &[f64], save_dir: &Path) -> Spec {
    let n = runs.len();
    Spec::new(MATRIX_KIND, MATRIX_STEM, save_dir)
        .set("title", "Adjusted Rand index between every pair of runs")
        .set("runs", serde_json::json!(runs))
        .set_f64_blob("values", values, &[n, n])
        // Fixed from zero to one rather than to the data: a matrix of runs that
        // all agree would otherwise be stretched into looking like one where
        // they do not.
        .set_f64_blob("domain", &[0.0, 1.0], &[2])
        .set("width", 1100)
        .set("height", 1000)
}

/// How well each niche of a reference run is found again in every other.
///
/// Rows are the reference's niches, columns the other runs. A row that stays
/// bright across the sweep is a niche that is a feature of the tissue; one that
/// darkens is a feature of the settings.
pub fn stability(
    reference: &str,
    niches: &[u32],
    runs: &[String],
    values: &[f64],
    save_dir: &Path,
) -> Spec {
    let labels: Vec<String> = niches.iter().map(|niche| niche.to_string()).collect();
    Spec::new(STABILITY_KIND, STABILITY_STEM, save_dir)
        .set(
            "title",
            format!("How the niches of run {reference} fare in the other runs"),
        )
        .set("niches", serde_json::json!(labels))
        .set("runs", serde_json::json!(runs))
        .set_f64_blob("values", values, &[niches.len(), runs.len()])
        .set_f64_blob("domain", &[0.0, 1.0], &[2])
        .set("width", 1100)
        .set("height", 900)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agreement_carries_both_measures_against_the_same_pairs() {
        let spec = agreement(
            &["1-1-1 → 1-1-2".to_string(), "1-1-2 → 1-1-3".to_string()],
            &[1.0, 0.4],
            &[1.0, 0.6],
            Path::new("/out"),
        );
        let json = spec.to_json();

        assert_eq!(json["pairs"].as_array().unwrap().len(), 2);
        assert_eq!(spec.blob_values("ari"), vec![1.0, 0.4]);
        assert_eq!(spec.blob_values("ami"), vec![1.0, 0.6]);
        assert_eq!(spec.stem(), AGREEMENT_STEM);

        // The two series are named here, as every other figure's colours are.
        assert_eq!(json["colours"], serde_json::json!([ARI_COLOUR, AMI_COLOUR]));
        assert_ne!(ARI_COLOUR, AMI_COLOUR);
    }

    #[test]
    fn the_matrix_is_square_and_says_so() {
        let runs = vec!["1-1-1".to_string(), "1-1-2".to_string()];
        let spec = matrix(&runs, &[1.0, 0.3, 0.3, 1.0], Path::new("/out"));
        let json = spec.to_json();

        assert_eq!(json["values"]["shape"], serde_json::json!([2, 2]));
        assert_eq!(spec.blob_values("values"), vec![1.0, 0.3, 0.3, 1.0]);
    }

    /// Fixed from zero to one: a matrix whose runs all agree would otherwise be
    /// stretched into looking like one where they do not.
    #[test]
    fn the_scales_are_fixed_rather_than_taken_from_the_data() {
        let spec = matrix(&["a".into()], &[1.0], Path::new("/out"));
        assert_eq!(spec.blob_values("domain"), vec![0.0, 1.0]);

        let spec = stability("1-1-1", &[0], &["1-1-2".into()], &[0.8], Path::new("/out"));
        assert_eq!(spec.blob_values("domain"), vec![0.0, 1.0]);
    }

    /// The rows are the reference's niches and the columns the other runs, and
    /// the title says which run the rows belong to — without it the figure is
    /// a grid of numbers about nothing in particular.
    #[test]
    fn the_stability_names_the_run_its_rows_belong_to() {
        let spec = stability(
            "1-1-1",
            &[0, 1, 7],
            &["1-1-2".to_string(), "1-1-3".to_string()],
            &[1.0, 0.5, 0.9, 0.2, 0.3, 0.1],
            Path::new("/out"),
        );
        let json = spec.to_json();

        assert!(
            json["title"].as_str().unwrap().contains("1-1-1"),
            "{}",
            json["title"]
        );
        assert_eq!(json["niches"], serde_json::json!(["0", "1", "7"]));
        assert_eq!(json["values"]["shape"], serde_json::json!([3, 2]));
    }

    /// The three kinds are distinct, or two of them would overwrite each other
    /// in the queue and the renderer would draw one twice.
    #[test]
    fn the_three_figures_are_told_apart() {
        let kinds = [AGREEMENT_KIND, MATRIX_KIND, STABILITY_KIND];
        let stems = [AGREEMENT_STEM, MATRIX_STEM, STABILITY_STEM];
        for pair in [kinds, stems] {
            let unique: std::collections::BTreeSet<&str> = pair.into_iter().collect();
            assert_eq!(unique.len(), 3, "{pair:?}");
        }
    }
}
