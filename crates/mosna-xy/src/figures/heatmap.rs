//! Every sample's z-scores, clustered on both axes.

use std::path::Path;

use mosna_core::colormap::rd_bu_r;
use mosna_core::stats::linkage::{dendrogram_leaf_order, ward_linkage};

use crate::dendrogram;
use crate::norm::TwoSlopeNorm;
use crate::palette;
use crate::spec::Spec;
use crate::table::Table;

pub const KIND: &str = "assortativity_heatmap";

/// With `include_self_pairs` false, the pairs of a phenotype with itself are
/// dropped: they are always strongly positive and their scale hides everything
/// else. That is the `_without_auto_paired_pheno` variant.
pub fn stem(include_self_pairs: bool) -> &'static str {
    if include_self_pairs {
        "Assortativity_heatmap_with_dendrogram"
    } else {
        "Assortativity_heatmap_with_dendrogram_without_auto_paired_pheno"
    }
}

pub fn spec(table: &Table, include_self_pairs: bool, save_dir: &Path) -> Spec {
    // Rows are phenotype pairs, columns are samples — the transpose the
    // original takes with `net_stat[assort_cols].T`.
    let pairs: Vec<(usize, String)> = table
        .pair_z_columns()
        .into_iter()
        .filter(|(_, column)| {
            include_self_pairs
                || Table::split_pair(column)
                    .map(|(left, right)| left != right)
                    .unwrap_or(true)
        })
        .collect();

    let n_rows = pairs.len();
    let n_cols = table.rows.len();

    let matrix: Vec<Vec<f64>> = (0..n_rows)
        .map(|row| {
            (0..n_cols)
                .map(|column| {
                    let value = table.value(column, pairs[row].0);
                    if value.is_finite() {
                        value
                    } else {
                        f64::NAN
                    }
                })
                .collect()
        })
        .collect();

    // Ward clustering on both axes, missing values treated as zero as the
    // original does with `.fillna(0)` before `pdist`.
    let rows_as_observations: Vec<Vec<f64>> = matrix
        .iter()
        .map(|row| row.iter().map(|v| finite_or_zero(*v)).collect())
        .collect();
    let columns_as_observations: Vec<Vec<f64>> = (0..n_cols)
        .map(|column| {
            (0..n_rows)
                .map(|row| finite_or_zero(matrix[row][column]))
                .collect()
        })
        .collect();

    let (row_order, row_tree) = cluster(&rows_as_observations);
    let (column_order, column_tree) = cluster(&columns_as_observations);

    let mut z = Vec::with_capacity(n_rows * n_cols);
    for &row in &row_order {
        for &column in &column_order {
            z.push(matrix[row][column]);
        }
    }

    let norm = TwoSlopeNorm::centred_on_zero(matrix.iter().flat_map(|row| row.iter().copied()));
    let colormap = palette::resample(
        &rd_bu_r(),
        |value| norm.normalise(value),
        norm.vmin(),
        norm.vmax(),
        palette::STOPS,
    );

    let y_labels: Vec<String> = row_order
        .iter()
        .map(|&row| {
            let name = &pairs[row].1;
            name.strip_suffix(" Z").unwrap_or(name).to_string()
        })
        .collect();
    let x_labels: Vec<String> = column_order
        .iter()
        .map(|&column| table.row_short_name(column))
        .collect();

    let row_segments: Vec<f64> = row_tree.iter().flatten().copied().collect();
    let column_segments: Vec<f64> = column_tree.iter().flatten().copied().collect();

    Spec::new(KIND, stem(include_self_pairs), save_dir)
        .set("title", "Assortativity heatmap by images")
        .set("colorbar_title", "z-score")
        .set("y_labels", serde_json::json!(y_labels))
        .set("x_labels", serde_json::json!(x_labels))
        .set("colormap", serde_json::json!(colormap))
        .set("domain", serde_json::json!([norm.vmin(), norm.vmax()]))
        .set("width", 2400)
        .set("height", 1500)
        .set_f64_blob("z", &z, &[n_rows, n_cols])
        .set_f64_blob("row_dendrogram", &row_segments, &[row_tree.len(), 4])
        .set_f64_blob(
            "column_dendrogram",
            &column_segments,
            &[column_tree.len(), 4],
        )
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// The leaf order Ward clustering puts the rows or the columns in, together
/// with the tree that produced it.
pub(crate) fn cluster(observations: &[Vec<f64>]) -> (Vec<usize>, Vec<dendrogram::Segment>) {
    if observations.len() < 2 {
        return ((0..observations.len()).collect(), Vec::new());
    }
    let linkage = ward_linkage(observations);
    let order = dendrogram_leaf_order(&linkage);
    let segments = dendrogram::segments(&linkage, &order);
    (order, segments)
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
                ("patient-1_sample-2".to_string(), vec![9.0, 1.0, -2.0, 0.5]),
                ("patient-3_sample-1".to_string(), vec![9.0, 0.2, -1.0, 0.1]),
                ("patient-4_sample-1".to_string(), vec![9.0, 3.0, 2.0, 0.4]),
            ],
        )
    }

    #[test]
    fn the_variant_without_self_pairs_has_its_own_file_and_fewer_rows() {
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);

        let all = spec(&table, true, Path::new("/out"));
        let without = spec(&table, false, Path::new("/out"));

        assert_eq!(all.stem(), "Assortativity_heatmap_with_dendrogram");
        assert!(without.stem().ends_with("_without_auto_paired_pheno"));
        assert_eq!(all.to_json()["z"]["shape"], serde_json::json!([3, 3]));
        assert_eq!(
            without.to_json()["z"]["shape"],
            serde_json::json!([1, 3]),
            "only A - B survives"
        );
    }

    /// `assort Z` is the network-wide coefficient, not a pair. Plotting it
    /// beside the pairs would put a value on a different scale into the same
    /// colour map.
    #[test]
    fn the_overall_coefficient_is_not_one_of_the_rows() {
        let (columns, rows) = table();
        let json = spec(&Table::new(&columns, &rows), true, Path::new("/out")).to_json();
        let labels: Vec<String> = serde_json::from_value(json["y_labels"].clone()).unwrap();

        assert!(!labels.iter().any(|label| label.contains("assort")));
        assert!(
            labels.iter().all(|label| !label.ends_with(" Z")),
            "{labels:?}"
        );
    }

    /// Rows are phenotype pairs and columns are samples: the transpose the
    /// original takes with `net_stat[assort_cols].T`.
    #[test]
    fn rows_are_pairs_and_columns_are_samples() {
        let (columns, rows) = table();
        let json = spec(&Table::new(&columns, &rows), true, Path::new("/out")).to_json();

        assert_eq!(json["z"]["shape"], serde_json::json!([3, 3]));
        let samples: Vec<String> = serde_json::from_value(json["x_labels"].clone()).unwrap();
        assert_eq!(samples.len(), 3);
        assert!(samples.contains(&"1-2".to_string()));
    }

    /// Both axes are ordered by the clustering, and both trees are handed over
    /// — that ordering is what makes the heatmap readable, and the tree is
    /// what says how much to believe it.
    #[test]
    fn both_axes_carry_their_tree() {
        let (columns, rows) = table();
        let json = spec(&Table::new(&columns, &rows), true, Path::new("/out")).to_json();

        assert_eq!(json["row_dendrogram"]["shape"][1], 4);
        assert_eq!(json["column_dendrogram"]["shape"][1], 4);
    }

    /// Zero has to land on the neutral centre of the map whatever the tails
    /// look like, which is the whole point of the diverging normalisation.
    #[test]
    fn the_colour_map_is_the_diverging_one_centred_on_zero() {
        let (columns, rows) = table();
        let spec = spec(&Table::new(&columns, &rows), true, Path::new("/out"));
        let json = spec.to_json();

        let stops: Vec<String> = serde_json::from_value(json["colormap"].clone()).unwrap();
        let domain: Vec<f64> = serde_json::from_value(json["domain"].clone()).unwrap();
        let norm = TwoSlopeNorm::new(domain[0], 0.0, domain[1]);

        let at_zero = ((0.0 - domain[0]) / (domain[1] - domain[0]) * (stops.len() - 1) as f64)
            .round() as usize;
        assert_eq!(stops[at_zero], palette::hex(rd_bu_r().sample(0.5)));
        assert!(norm.normalise(0.0) == 0.5);
    }

    #[test]
    fn an_empty_table_is_still_a_specification() {
        let json = spec(&Table::new(&[], &[]), true, Path::new("/out")).to_json();
        assert_eq!(json["z"]["shape"], serde_json::json!([0, 0]));
    }
}
