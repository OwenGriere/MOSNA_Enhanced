//! Per-sample mixing matrix heatmaps — port of
//! `assort_figures_mixing_matrix.py` and its `_without_diag` twin.

use std::path::Path;

use plotters::prelude::*;

use mosna_core::assortativity::series_to_mixmat;

use crate::assortativity::Table;
use crate::canvas::{colour, figure, label_style, title_style};
use crate::colormap::{rd_bu_r, Gradient};
use crate::norm::TwoSlopeNorm;
use crate::theme::Theme;

const WIDTH_INCHES: f64 = 24.0;
const HEIGHT_INCHES: f64 = 15.0;

/// Draw one heatmap per sample into `assort_files`, and a second set with the
/// diagonal blanked into `assort_files_without_diag`.
///
/// The diagonal — a phenotype against itself — is always the strongest signal
/// and squashes the colour scale; blanking it is what makes the off-diagonal
/// structure visible.
pub fn draw_all(theme: &Theme, table: &Table, save_dir: &Path) -> anyhow::Result<()> {
    for (folder, blank_diagonal) in [("assort_files", false), ("assort_files_without_diag", true)] {
        let target = save_dir.join(folder);
        std::fs::create_dir_all(&target)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", target.display()))?;

        for sample in 0..table.rows.len() {
            draw_one(theme, table, sample, blank_diagonal, &target)?;
        }
    }
    Ok(())
}

fn draw_one(
    theme: &Theme,
    table: &Table,
    sample: usize,
    blank_diagonal: bool,
    save_dir: &Path,
) -> anyhow::Result<()> {
    let path = save_dir.join(format!(
        "heatmap_zscore_{}.png",
        table.row_short_name(sample)
    ));

    let pairs = table.pair_z_columns();
    let names: Vec<String> = pairs.iter().map(|(_, name)| name.clone()).collect();
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

    // The overall coefficient goes in the title, as the Python does.
    let overall = table
        .columns
        .iter()
        .position(|name| name == "assort Z")
        .map(|index| table.value(sample, index))
        .unwrap_or(f64::NAN);

    figure(&path, theme, WIDTH_INCHES, HEIGHT_INCHES, |root| {
        let (title_area, body) = root.split_vertically(theme.font(25.0) * 2);
        title_area
            .titled(
                &format!("Z-score heatmap with a general assortativity: {overall}"),
                title_style(theme, 25.0),
            )
            .map_err(|e| anyhow::anyhow!("cannot draw the title: {e}"))?;

        if matrix.n == 0 {
            return Ok(());
        }

        let norm =
            TwoSlopeNorm::centred_on_zero(matrix.values.iter().copied().filter(|v| v.is_finite()));
        let map = rd_bu_r();

        let label_width = (body.dim_in_pixel().0 as f64 * 0.18) as u32;
        let axis_height = theme.font(10.0) * 10;
        let (labels_area, plot_area) = body.split_horizontally(label_width);
        let (plot, axis) =
            plot_area.split_vertically(plot_area.dim_in_pixel().1.saturating_sub(axis_height));
        let (row_labels, _) =
            labels_area.split_vertically(labels_area.dim_in_pixel().1.saturating_sub(axis_height));

        let (plot_width, plot_height) = plot.dim_in_pixel();
        let cell = (plot_width as f64 / matrix.n as f64).min(plot_height as f64 / matrix.n as f64);

        for row in 0..matrix.n {
            for column in 0..matrix.n {
                let value = matrix.get(row, column);
                let shade = if value.is_finite() {
                    map.sample(norm.normalise(value))
                } else {
                    Gradient::BAD
                };
                let x0 = (column as f64 * cell).round() as i32;
                let y0 = (row as f64 * cell).round() as i32;
                let x1 = ((column + 1) as f64 * cell).ceil() as i32;
                let y1 = ((row + 1) as f64 * cell).ceil() as i32;
                plot.draw(&Rectangle::new(
                    [(x0, y0), (x1, y1)],
                    colour(shade).filled(),
                ))
                .map_err(|e| anyhow::anyhow!("cannot draw a cell: {e}"))?;
            }
        }

        let style = label_style(theme, 10.0);
        let rotated = crate::canvas::rotated_label_style(theme, 10.0);
        for (index, label) in labels.iter().enumerate() {
            let position = (index as f64 * cell + cell / 2.0).round() as i32;
            row_labels
                .draw(&Text::new(label.clone(), (4, position), style.clone()))
                .map_err(|e| anyhow::anyhow!("cannot draw a row label: {e}"))?;
            axis.draw(&Text::new(label.clone(), (position, 4), rotated.clone()))
                .map_err(|e| anyhow::anyhow!("cannot draw a column label: {e}"))?;
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
        let columns = vec![
            "assort Z".to_string(),
            "A - A Z".to_string(),
            "A - B Z".to_string(),
            "B - B Z".to_string(),
        ];
        let rows = vec![
            ("patient-1_sample-2".to_string(), vec![2.0, 5.0, -3.0, 4.0]),
            ("patient-2_sample-1".to_string(), vec![1.0, 3.0, -1.0, 2.0]),
        ];
        (columns, rows)
    }

    #[test]
    fn one_figure_per_sample_in_each_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (columns, rows) = table();
        draw_all(
            &Theme { dpi: 12.0 },
            &Table::new(&columns, &rows),
            dir.path(),
        )
        .unwrap();

        for folder in ["assort_files", "assort_files_without_diag"] {
            let target = dir.path().join(folder);
            assert!(target.join("heatmap_zscore_1-2.png").is_file());
            assert!(target.join("heatmap_zscore_2-1.png").is_file());
        }
    }

    #[test]
    fn an_empty_table_creates_the_folders_but_no_figure() {
        let dir = tempfile::tempdir().unwrap();
        draw_all(&Theme { dpi: 12.0 }, &Table::new(&[], &[]), dir.path()).unwrap();
        assert!(dir.path().join("assort_files").is_dir());
        assert_eq!(
            std::fs::read_dir(dir.path().join("assort_files"))
                .unwrap()
                .count(),
            0
        );
    }
}
