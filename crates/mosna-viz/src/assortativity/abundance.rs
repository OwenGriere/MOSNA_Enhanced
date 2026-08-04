//! Relative abundance of each phenotype per sample — port of
//! `assort_figures_abundance.py`.

use std::path::Path;

use plotters::prelude::*;

use crate::assortativity::Table;
use crate::canvas::{colour, figure, label_style, title_style};
use crate::colormap::abundance_palette;
use crate::theme::Theme;

const WIDTH_INCHES: f64 = 18.0;
const HEIGHT_INCHES: f64 = 9.0;

/// Draw the stacked bar chart of phenotype proportions.
pub fn draw(theme: &Theme, table: &Table, save_dir: &Path) -> anyhow::Result<()> {
    let path = save_dir.join("abundance.png");
    let phenotypes = table.abundance_columns();

    figure(&path, theme, WIDTH_INCHES, HEIGHT_INCHES, |root| {
        let (title_area, body) = root.split_vertically(theme.font(25.0) * 2);
        title_area
            .titled(
                "Abondance relative des types cellulaires par sample",
                title_style(theme, 25.0),
            )
            .map_err(|e| anyhow::anyhow!("cannot draw the title: {e}"))?;

        if table.is_empty() || phenotypes.is_empty() {
            return Ok(());
        }

        let palette = abundance_palette(phenotypes.len());
        let n_samples = table.rows.len();

        // Room for the legend on the right, as `bbox_to_anchor=(1.02, 1)` does.
        let legend_width = (body.dim_in_pixel().0 as f64 * 0.18) as u32;
        let (plot_area, legend) =
            body.split_horizontally(body.dim_in_pixel().0.saturating_sub(legend_width));

        let mut chart = ChartBuilder::on(&plot_area)
            .margin(theme.stroke(8.0))
            .x_label_area_size(theme.font(20.0) * 3)
            .y_label_area_size(theme.font(20.0) * 3)
            .build_cartesian_2d(-0.5f64..(n_samples as f64 - 0.5), 0f64..1.0f64)
            .map_err(|e| anyhow::anyhow!("cannot build the abundance axes: {e}"))?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_desc("Sample")
            .y_desc("Proportion")
            .x_labels(n_samples.min(30))
            .x_label_formatter(&|position| {
                let index = position.round();
                if index < 0.0 {
                    return String::new();
                }
                table.row_short_name(index as usize)
            })
            .axis_desc_style(label_style(theme, 20.0))
            .label_style(label_style(theme, 10.0))
            .draw()
            .map_err(|e| anyhow::anyhow!("cannot draw the abundance mesh: {e}"))?;

        for (sample, _) in table.rows.iter().enumerate() {
            // `plot_df.div(plot_df.sum(axis=1), axis=0)`: each bar is a
            // composition, so a sample whose proportions do not already sum to
            // one is rescaled rather than drawn short.
            let values: Vec<f64> = phenotypes
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
            let total: f64 = values.iter().sum();
            if total <= 0.0 {
                continue;
            }

            let mut base = 0.0f64;
            for (index, value) in values.iter().enumerate() {
                let height = value / total;
                if height <= 0.0 {
                    continue;
                }
                let position = sample as f64;
                chart
                    .draw_series(std::iter::once(Rectangle::new(
                        [(position - 0.4, base), (position + 0.4, base + height)],
                        colour(palette[index]).filled(),
                    )))
                    .map_err(|e| anyhow::anyhow!("cannot draw a bar segment: {e}"))?;
                base += height;
            }
        }

        draw_legend(theme, &legend, &phenotypes, &palette)?;
        Ok(())
    })
}

fn draw_legend(
    theme: &Theme,
    area: &crate::canvas::Surface<'_>,
    phenotypes: &[(usize, String)],
    palette: &[crate::colormap::Rgb],
) -> anyhow::Result<()> {
    let (_, height) = area.dim_in_pixel();
    let font = theme.font(8.0);
    let step = (font as f64 * 1.9) as i32;
    let swatch = (font as f64 * 0.9) as i32;
    let mut y = step;

    // The Python reverses the legend so it reads in the same order as the
    // stack, whose first phenotype is at the bottom.
    for (index, (_, phenotype)) in phenotypes.iter().enumerate().rev() {
        if y + step > height as i32 {
            break;
        }
        area.draw(&Rectangle::new(
            [(swatch, y), (swatch * 2, y + swatch)],
            colour(palette[index]).filled(),
        ))
        .map_err(|e| anyhow::anyhow!("cannot draw a legend swatch: {e}"))?;
        area.draw(&Text::new(
            phenotype.clone(),
            (swatch * 3, y),
            label_style(theme, 8.0),
        ))
        .map_err(|e| anyhow::anyhow!("cannot draw a legend label: {e}"))?;
        y += step;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_figure_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let columns = vec!["% A".to_string(), "% B".to_string()];
        let rows = vec![
            ("patient-1_sample-1".to_string(), vec![0.7, 0.3]),
            ("patient-2_sample-1".to_string(), vec![0.4, 0.6]),
        ];
        draw(
            &Theme { dpi: 20.0 },
            &Table::new(&columns, &rows),
            dir.path(),
        )
        .unwrap();
        assert!(dir.path().join("abundance.png").is_file());
    }

    #[test]
    fn a_sample_with_no_cells_is_skipped_rather_than_dividing_by_zero() {
        let dir = tempfile::tempdir().unwrap();
        let columns = vec!["% A".to_string()];
        let rows = vec![("patient-1".to_string(), vec![0.0])];
        draw(
            &Theme { dpi: 20.0 },
            &Table::new(&columns, &rows),
            dir.path(),
        )
        .unwrap();
        assert!(dir.path().join("abundance.png").is_file());
    }

    #[test]
    fn an_empty_table_still_writes_a_figure() {
        let dir = tempfile::tempdir().unwrap();
        draw(&Theme { dpi: 20.0 }, &Table::new(&[], &[]), dir.path()).unwrap();
        assert!(dir.path().join("abundance.png").is_file());
    }
}
