//! The niche composition heatmap.

use std::path::Path;

use plotters::prelude::*;

use mosna_core::niches::{NicheComposition, Normalize};

use crate::canvas::{colour, figure, label_style, title_style};
use crate::colormap::blues;
use crate::theme::{Theme, EDGE};

/// Width in inches, from `plt.figure(figsize=(20, fig_height))`.
const WIDTH_INCHES: f64 = 20.0;

/// Height in inches: `max(8, n_phenotypes * 0.35)`, so a cohort with many cell
/// types gets a taller figure rather than unreadable rows.
fn height_inches(n_phenotypes: usize) -> f64 {
    (n_phenotypes as f64 * 0.35).max(8.0)
}

/// Draw the phenotype-by-niche heatmap.
///
/// The file name carries the normalisation, because `normalize: all` produces
/// one figure per variant into the same directory.
pub fn draw(
    theme: &Theme,
    composition: &NicheComposition,
    normalize: Normalize,
    save_dir: &Path,
) -> anyhow::Result<()> {
    let path = save_dir.join(format!(
        "Niches_Aggregated_Composition_{}.png",
        normalize.as_str()
    ));

    let n_rows = composition.phenotypes.len();
    let n_cols = composition.niches.len();

    figure(&path, theme, WIDTH_INCHES, height_inches(n_rows), |root| {
        let (title_area, body) = root.split_vertically(theme.font(14.0) * 3);
        title_area
            .titled("Niches Aggregated Composition", title_style(theme, 14.0))
            .map_err(|e| anyhow::anyhow!("cannot draw the title: {e}"))?;

        if n_rows == 0 || n_cols == 0 {
            return Ok(());
        }

        // Room for the phenotype names on the left and the niche ids below.
        let label_width = (body.dim_in_pixel().0 as f64 * 0.22) as u32;
        let axis_height = theme.font(10.0) * 3;
        let (labels_area, plot_area) = body.split_horizontally(label_width);
        let (plot, axis) =
            plot_area.split_vertically(plot_area.dim_in_pixel().1.saturating_sub(axis_height));
        let (row_labels, _) =
            labels_area.split_vertically(labels_area.dim_in_pixel().1.saturating_sub(axis_height));

        // The map is scaled to the data, as seaborn does by default.
        let (low, high) = composition
            .counts
            .iter()
            .filter(|v| v.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(*v), hi.max(*v))
            });
        let span = if high > low { high - low } else { 1.0 };
        let map = blues();

        let (plot_width, plot_height) = plot.dim_in_pixel();
        let cell_width = plot_width as f64 / n_cols as f64;
        let cell_height = plot_height as f64 / n_rows as f64;

        for row in 0..n_rows {
            for column in 0..n_cols {
                let value = composition.get(row, column);
                let shade = if value.is_finite() {
                    map.sample((value - low) / span)
                } else {
                    crate::colormap::Gradient::BAD
                };

                let x0 = (column as f64 * cell_width).round() as i32;
                let y0 = (row as f64 * cell_height).round() as i32;
                let x1 = ((column + 1) as f64 * cell_width).round() as i32;
                let y1 = ((row + 1) as f64 * cell_height).round() as i32;

                plot.draw(&Rectangle::new(
                    [(x0, y0), (x1, y1)],
                    colour(shade).filled(),
                ))
                .map_err(|e| anyhow::anyhow!("cannot draw a cell: {e}"))?;
                // The thin separator seaborn draws with `linewidths=.5`.
                plot.draw(&Rectangle::new(
                    [(x0, y0), (x1, y1)],
                    colour(EDGE).stroke_width(1),
                ))
                .map_err(|e| anyhow::anyhow!("cannot outline a cell: {e}"))?;
            }
        }

        let style = label_style(theme, 8.0);
        for (row, phenotype) in composition.phenotypes.iter().enumerate() {
            let y = (row as f64 * cell_height + cell_height / 2.0).round() as i32;
            row_labels
                .draw(&Text::new(phenotype.clone(), (4, y), style.clone()))
                .map_err(|e| anyhow::anyhow!("cannot draw a row label: {e}"))?;
        }

        let style = label_style(theme, 10.0);
        for (column, niche) in composition.niches.iter().enumerate() {
            let x = (column as f64 * cell_width + cell_width / 2.0).round() as i32;
            axis.draw(&Text::new(niche.to_string(), (x, 4), style.clone()))
                .map_err(|e| anyhow::anyhow!("cannot draw a column label: {e}"))?;
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_core::niches::make_niches_composition;

    fn composition() -> NicheComposition {
        let cell_types: Vec<String> = (0..30)
            .map(|i| ["A", "B", "C"][i % 3].to_string())
            .collect();
        let niches: Vec<u32> = (0..30).map(|i| (i / 10) as u32).collect();
        make_niches_composition(&cell_types, &niches, Normalize::Total).unwrap()
    }

    #[test]
    fn the_height_grows_with_the_phenotype_count() {
        assert_eq!(height_inches(3), 8.0, "a small cohort keeps the floor");
        assert!((height_inches(40) - 14.0).abs() < 1e-9);
    }

    #[test]
    fn the_file_name_carries_the_normalisation() {
        let dir = tempfile::tempdir().unwrap();
        for normalize in [Normalize::Total, Normalize::Clr] {
            draw(&Theme { dpi: 20.0 }, &composition(), normalize, dir.path()).unwrap();
            assert!(dir
                .path()
                .join(format!(
                    "Niches_Aggregated_Composition_{}.png",
                    normalize.as_str()
                ))
                .is_file());
        }
    }

    #[test]
    fn an_empty_composition_still_writes_a_figure() {
        let dir = tempfile::tempdir().unwrap();
        let empty = make_niches_composition(&[], &[], Normalize::Total).unwrap();
        draw(&Theme { dpi: 20.0 }, &empty, Normalize::Total, dir.path()).unwrap();
        assert!(dir
            .path()
            .join("Niches_Aggregated_Composition_total.png")
            .is_file());
    }
}
