//! Relative abundance of each phenotype, per sample.

use std::path::Path;

use mosna_core::colormap::abundance_palette;

use crate::palette::hex;
use crate::spec::Spec;
use crate::table::Table;

pub const KIND: &str = "abundance";
pub const STEM: &str = "abundance";

pub fn spec(table: &Table, save_dir: &Path) -> Spec {
    let phenotypes = table.abundance_columns();
    let samples: Vec<String> = (0..table.rows.len())
        .map(|row| table.row_short_name(row))
        .collect();

    // Row-major, phenotypes down and samples across, which is how the renderer
    // stacks one band per phenotype.
    let mut values = vec![0.0f64; phenotypes.len() * samples.len()];
    for (sample, _) in table.rows.iter().enumerate() {
        let column: Vec<f64> = phenotypes
            .iter()
            .map(|(index, _)| {
                let value = table.value(sample, *index);
                if value.is_finite() {
                    value.max(0.0)
                } else {
                    0.0
                }
            })
            .collect();

        // `plot_df.div(plot_df.sum(axis=1), axis=0)`: each bar is a
        // composition, so a sample whose proportions do not already sum to one
        // is rescaled rather than drawn short. A sample measuring nothing
        // cannot be rescaled and is left empty.
        let total: f64 = column.iter().sum();
        if total <= 0.0 {
            continue;
        }
        for (phenotype, value) in column.iter().enumerate() {
            values[phenotype * samples.len() + sample] = value / total;
        }
    }

    let names: Vec<String> = phenotypes.iter().map(|(_, name)| name.clone()).collect();
    let colours: Vec<String> = abundance_palette(phenotypes.len())
        .into_iter()
        .map(hex)
        .collect();

    Spec::new(KIND, STEM, save_dir)
        .set(
            "title",
            "Abondance relative des types cellulaires par sample",
        )
        .set("samples", serde_json::json!(samples))
        .set("phenotypes", serde_json::json!(names))
        .set("colours", serde_json::json!(colours))
        .set("width", 1800)
        .set("height", 900)
        .set_f64_blob("values", &values, &[phenotypes.len(), samples.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
        (
            vec!["# total".into(), "% A".into(), "% B".into()],
            vec![
                ("patient-1_sample-2".to_string(), vec![10.0, 30.0, 10.0]),
                ("patient-3_sample-1".to_string(), vec![20.0, 25.0, 25.0]),
            ],
        )
    }

    #[test]
    fn the_bands_are_the_abundance_columns_and_the_bars_are_the_samples() {
        let (columns, rows) = table();
        let json = spec(&Table::new(&columns, &rows), Path::new("/out")).to_json();

        assert_eq!(json["phenotypes"], serde_json::json!(["A", "B"]));
        assert_eq!(json["samples"], serde_json::json!(["1-2", "3-1"]));
        assert_eq!(json["values"]["shape"], serde_json::json!([2, 2]));
    }

    /// `plot_df.div(plot_df.sum(axis=1), axis=0)`: each bar is a composition,
    /// so a sample whose proportions do not already sum to one is rescaled
    /// rather than drawn short.
    #[test]
    fn every_bar_is_rescaled_to_a_whole() {
        let (columns, rows) = table();
        let spec = spec(&Table::new(&columns, &rows), Path::new("/out"));
        let values = spec.blob_values("values");

        // Sample one: 30 and 10 become 0.75 and 0.25.
        assert!((values[0] - 0.75).abs() < 1e-12, "{values:?}");
        assert!((values[2] - 0.25).abs() < 1e-12, "{values:?}");
    }

    /// A sample with nothing measured cannot be rescaled; it is left at zero
    /// rather than filled with `NaN`, which would draw as a gap in the bar.
    #[test]
    fn a_sample_measuring_nothing_is_left_empty() {
        let columns = vec!["% A".to_string(), "% B".to_string()];
        let rows = vec![("patient-1".to_string(), vec![0.0, 0.0])];
        let spec = spec(&Table::new(&columns, &rows), Path::new("/out"));

        assert!(spec.blob_values("values").iter().all(|v| *v == 0.0));
    }

    #[test]
    fn the_bands_carry_the_twenty_colour_palette() {
        let (columns, rows) = table();
        let json = spec(&Table::new(&columns, &rows), Path::new("/out")).to_json();
        assert_eq!(json["colours"][0], hex(abundance_palette(2)[0]));
    }
}
