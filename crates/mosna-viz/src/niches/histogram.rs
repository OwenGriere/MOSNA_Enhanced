//! The niche size histogram.

use std::path::Path;

use plotters::prelude::*;

use crate::canvas::{colour, figure, title_style};
use crate::colormap::make_cluster_cmap;
use crate::theme::Theme;

/// Figure size in inches, from `plt.figure(figsize=(20, 8))`.
const WIDTH_INCHES: f64 = 20.0;
const HEIGHT_INCHES: f64 = 8.0;

/// Draw how many cells each niche holds.
///
/// Bars are coloured with the cluster palette, so a niche keeps the same colour
/// here as in the embedding and the network re-plot.
pub fn draw(theme: &Theme, niches: &[u32], save_dir: &Path) -> anyhow::Result<()> {
    let path = save_dir.join("Niches_Histogram.png");

    let mut counts: std::collections::BTreeMap<u32, usize> = Default::default();
    for niche in niches {
        *counts.entry(*niche).or_insert(0) += 1;
    }
    let bars: Vec<(u32, usize)> = counts.into_iter().collect();

    figure(&path, theme, WIDTH_INCHES, HEIGHT_INCHES, |root| {
        let (title_area, body) = root.split_vertically(theme.font(14.0) * 3);
        title_area
            .titled("Niches histogram", title_style(theme, 14.0))
            .map_err(|e| anyhow::anyhow!("cannot draw the title: {e}"))?;

        if bars.is_empty() {
            return Ok(());
        }

        let tallest = bars.iter().map(|(_, count)| *count).max().unwrap_or(1);
        let palette = make_cluster_cmap(bars.len());

        let mut chart = ChartBuilder::on(&body)
            .margin(theme.stroke(10.0))
            .x_label_area_size(theme.font(11.0) * 3)
            .y_label_area_size(theme.font(11.0) * 4)
            // Half a unit of padding either side keeps the outer bars off the
            // frame, which `ax.bar(width=0.8)` gets from matplotlib's margins.
            .build_cartesian_2d(
                -0.5f64..(bars.len() as f64 - 0.5),
                0f64..(tallest as f64 * 1.05),
            )
            .map_err(|e| anyhow::anyhow!("cannot build the histogram axes: {e}"))?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_labels(bars.len().min(30))
            .x_label_formatter(&|position| {
                let index = position.round() as usize;
                bars.get(index)
                    .map(|(niche, _)| niche.to_string())
                    .unwrap_or_default()
            })
            .label_style(crate::canvas::label_style(theme, 11.0))
            .draw()
            .map_err(|e| anyhow::anyhow!("cannot draw the histogram mesh: {e}"))?;

        chart
            .draw_series(bars.iter().enumerate().map(|(index, (_, count))| {
                let position = index as f64;
                Rectangle::new(
                    [(position - 0.4, 0.0), (position + 0.4, *count as f64)],
                    colour(palette[index]).filled(),
                )
            }))
            .map_err(|e| anyhow::anyhow!("cannot draw the bars: {e}"))?;

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_figure_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let niches: Vec<u32> = (0..50).map(|i| (i % 4) as u32).collect();
        draw(&Theme { dpi: 20.0 }, &niches, dir.path()).unwrap();
        assert!(dir.path().join("Niches_Histogram.png").is_file());
    }

    #[test]
    fn an_empty_labelling_still_writes_a_figure() {
        let dir = tempfile::tempdir().unwrap();
        draw(&Theme { dpi: 20.0 }, &[], dir.path()).unwrap();
        assert!(dir.path().join("Niches_Histogram.png").is_file());
    }

    #[test]
    fn a_single_niche_does_not_collapse_the_axes() {
        let dir = tempfile::tempdir().unwrap();
        draw(&Theme { dpi: 20.0 }, &[0, 0, 0], dir.path()).unwrap();
        assert!(dir.path().join("Niches_Histogram.png").is_file());
    }
}
