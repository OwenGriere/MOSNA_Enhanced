//! One heatmap per sample: its phenotype-by-phenotype mixing matrix.

use std::path::Path;

use mosna_core::assortativity::series_to_mixmat;
use mosna_core::colormap::rd_bu_r;

use crate::norm::TwoSlopeNorm;
use crate::palette;
use crate::spec::Spec;
use crate::table::Table;

pub const KIND: &str = "mixing_matrix";

/// The two directories the per-sample heatmaps go into.
///
/// The diagonal — a phenotype against itself — is always the strongest signal
/// and squashes the colour scale; blanking it is what makes the off-diagonal
/// structure visible, and both versions are worth having.
pub const FOLDERS: [(&str, bool); 2] =
    [("assort_files", false), ("assort_files_without_diag", true)];

pub fn specs(table: &Table, save_dir: &Path) -> Vec<Spec> {
    let pairs = table.pair_z_columns();
    let names: Vec<String> = pairs.iter().map(|(_, name)| name.clone()).collect();

    // The overall coefficient goes in the title, as the original does: it is
    // the one number that describes the whole sample.
    let overall_column = table.columns.iter().position(|name| name == "assort Z");

    let mut specs = Vec::with_capacity(FOLDERS.len() * table.rows.len());
    for (folder, blank_diagonal) in FOLDERS {
        for sample in 0..table.rows.len() {
            let values: Vec<f64> = pairs
                .iter()
                .map(|(index, _)| table.value(sample, *index))
                .collect();
            let (labels, mut matrix) = series_to_mixmat(&names, &values, " - ", " Z");

            if blank_diagonal {
                for i in 0..matrix.n {
                    matrix.set(i, i, f64::NAN);
                }
            }

            let overall = overall_column
                .map(|column| table.value(sample, column))
                .unwrap_or(f64::NAN);

            let norm = TwoSlopeNorm::centred_on_zero(matrix.values.iter().copied());
            let colormap = palette::resample(
                &rd_bu_r(),
                |value| norm.normalise(value),
                norm.vmin(),
                norm.vmax(),
                palette::STOPS,
            );

            specs.push(
                Spec::new(
                    KIND,
                    format!("heatmap_zscore_{}", table.row_short_name(sample)),
                    &save_dir.join(folder),
                )
                .set(
                    "title",
                    format!("Z-score heatmap with a general assortativity: {overall}"),
                )
                .set("colorbar_title", "z-score")
                .set("x_labels", serde_json::json!(labels))
                .set("y_labels", serde_json::json!(labels))
                .set("colormap", serde_json::json!(colormap))
                .set("domain", serde_json::json!([norm.vmin(), norm.vmax()]))
                .set("width", 2400)
                .set("height", 1500)
                .set_f64_blob("z", &matrix.values, &[matrix.n, matrix.n]),
            );
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
        (
            vec![
                "assort Z".into(),
                "A - A Z".into(),
                "A - B Z".into(),
                "B - B Z".into(),
            ],
            vec![
                ("patient-1_sample-2".to_string(), vec![7.5, 1.0, -2.0, 0.5]),
                ("patient-3_sample-1".to_string(), vec![1.5, 0.2, -1.0, 0.1]),
            ],
        )
    }

    #[test]
    fn there_are_two_figures_for_every_sample() {
        let (columns, rows) = table();
        let specs = specs(&Table::new(&columns, &rows), Path::new("/out"));
        assert_eq!(specs.len(), 4);
    }

    /// The interface parses `heatmap_zscore_{patient}-{sample}` to group its
    /// gallery by patient, so both the name and the directory are contracts.
    #[test]
    fn each_is_named_and_filed_the_way_the_gallery_reads_it() {
        let (columns, rows) = table();
        let specs = specs(&Table::new(&columns, &rows), Path::new("/out"));
        let json: Vec<_> = specs.iter().map(|spec| spec.to_json()).collect();

        assert_eq!(specs[0].stem(), "heatmap_zscore_1-2");
        assert_eq!(json[0]["save_dir"], "/out/assort_files");
        assert!(json
            .iter()
            .any(|spec| spec["save_dir"] == "/out/assort_files_without_diag"));
    }

    #[test]
    fn the_second_set_has_its_diagonal_blanked() {
        let (columns, rows) = table();
        let specs = specs(&Table::new(&columns, &rows), Path::new("/out"));

        let with_diagonal = specs
            .iter()
            .find(|spec| spec.to_json()["save_dir"] == "/out/assort_files")
            .unwrap();
        let without = specs
            .iter()
            .find(|spec| spec.to_json()["save_dir"] == "/out/assort_files_without_diag")
            .unwrap();

        assert!(with_diagonal.blob_values("z")[0].is_finite());
        assert!(without.blob_values("z")[0].is_nan());
    }

    /// The overall coefficient goes in the title, as the original does: it is
    /// the one number that describes the whole sample.
    #[test]
    fn the_title_carries_the_general_coefficient_of_that_sample() {
        let (columns, rows) = table();
        let specs = specs(&Table::new(&columns, &rows), Path::new("/out"));
        let title = specs[0].to_json()["title"].as_str().unwrap().to_string();

        assert!(title.contains("7.5"), "{title}");
    }

    #[test]
    fn a_table_with_no_samples_produces_no_figures() {
        assert!(specs(&Table::new(&[], &[]), Path::new("/out")).is_empty());
    }
}
