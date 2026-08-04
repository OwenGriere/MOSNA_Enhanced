//! The spatial network figure — port of `core/tysserand/draw_per_sample.py`
//! and `tysserand::plot_network`.

use std::path::Path;

use plotters::prelude::*;

use mosna_io::SampleId;

use crate::canvas::{colour, figure, title_style};
use crate::colormap::make_cluster_cmap;
use crate::theme::Theme;

/// Figure size in inches, from `plot_network(figsize=(30, 30))`.
const SIZE_INCHES: f64 = 30.0;
/// Fraction of the width the legend occupies on the right.
const LEGEND_FRACTION: f64 = 0.18;

/// Draw one sample's network, coloured by phenotype.
///
/// Edges are black at 80 % opacity, nodes are filled circles, and the legend
/// lists the phenotypes — the same arrangement as the Python, whose edge
/// styling is applied after the fact in `draw_per_sample`.
///
/// The file is `net_{patient}.png` or `net_{patient}-{sample}.png`; the
/// interface groups its gallery by parsing that name, so it is a contract.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    theme: &Theme,
    sample: &SampleId,
    patient_column: &str,
    sample_column: Option<&str>,
    coords: &[[f64; 2]],
    pairs: &[(u32, u32)],
    labels: &[String],
    save_dir: &Path,
) -> anyhow::Result<()> {
    let name = match (&sample.sample, sample_column) {
        (Some(id), Some(_)) => format!("net_{}-{}.png", sample.patient, id),
        _ => format!("net_{}.png", sample.patient),
    };
    let path = save_dir.join(name);

    let title = match (&sample.sample, sample_column) {
        (Some(id), Some(column)) => format!(
            "Tysserand network {patient_column} {} and {column} {id}",
            sample.patient
        ),
        _ => format!("Tysserand network {patient_column} {}", sample.patient),
    };

    // The phenotype vocabulary, in first-seen order, so a sample's colours do
    // not depend on how its cells happen to be sorted.
    let mut vocabulary: Vec<&str> = Vec::new();
    for label in labels {
        if !vocabulary.contains(&label.as_str()) {
            vocabulary.push(label);
        }
    }
    let palette = make_cluster_cmap(vocabulary.len());

    figure(&path, theme, SIZE_INCHES, SIZE_INCHES, |root| {
        let (title_area, body) = root.split_vertically(theme.font(30.0) * 2);
        title_area
            .titled(&title, title_style(theme, 30.0))
            .map_err(|e| anyhow::anyhow!("cannot draw the title: {e}"))?;

        let legend_width = (body.dim_in_pixel().0 as f64 * LEGEND_FRACTION) as u32;
        let (plot, legend) =
            body.split_horizontally(body.dim_in_pixel().0.saturating_sub(legend_width));

        if coords.is_empty() {
            return Ok(());
        }

        let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
        for point in coords {
            min_x = min_x.min(point[0]);
            max_x = max_x.max(point[0]);
            min_y = min_y.min(point[1]);
            max_y = max_y.max(point[1]);
        }
        // A margin keeps the outermost cells off the frame.
        let pad = ((max_x - min_x).max(max_y - min_y) * 0.03).max(1e-6);

        let mut chart = ChartBuilder::on(&plot)
            .margin(theme.stroke(10.0))
            .build_cartesian_2d(min_x - pad..max_x + pad, min_y - pad..max_y + pad)
            .map_err(|e| anyhow::anyhow!("cannot build the network axes: {e}"))?;

        // Edges first, so the nodes sit on top of them as in the Python, where
        // the nodes are drawn at `zorder=10` and the edges at `zorder=5`.
        let edge_colour = RGBColor(0, 0, 0).mix(0.8);
        let edge_width = theme.stroke(0.6);
        chart
            .draw_series(pairs.iter().filter_map(|&(a, b)| {
                let (a, b) = (a as usize, b as usize);
                let (from, to) = (coords.get(a)?, coords.get(b)?);
                Some(PathElement::new(
                    vec![(from[0], from[1]), (to[0], to[1])],
                    edge_colour.stroke_width(edge_width),
                ))
            }))
            .map_err(|e| anyhow::anyhow!("cannot draw the edges: {e}"))?;

        let node_radius = theme.stroke(3.0) as i32;
        for (index, phenotype) in vocabulary.iter().enumerate() {
            let fill = colour(palette[index]).filled();
            chart
                .draw_series(
                    coords
                        .iter()
                        .zip(labels)
                        .filter(|(_, label)| label.as_str() == *phenotype)
                        .map(|(point, _)| Circle::new((point[0], point[1]), node_radius, fill)),
                )
                .map_err(|e| anyhow::anyhow!("cannot draw the nodes: {e}"))?;
        }

        draw_legend(theme, &legend, &vocabulary, &palette)?;
        Ok(())
    })
}

/// A vertical list of swatches and phenotype names.
fn draw_legend(
    theme: &Theme,
    area: &crate::canvas::Surface<'_>,
    vocabulary: &[&str],
    palette: &[crate::colormap::Rgb],
) -> anyhow::Result<()> {
    if vocabulary.is_empty() {
        return Ok(());
    }

    let (width, height) = area.dim_in_pixel();
    let font = theme.font(14.0);
    let step = (font as f64 * 1.8) as i32;
    let swatch = (font as f64 * 0.8) as i32;

    // Centred vertically, like the Python's `loc='center left'`.
    let total = step * vocabulary.len() as i32;
    let mut y = ((height as i32 - total) / 2).max(step);

    for (index, phenotype) in vocabulary.iter().enumerate() {
        if y + step > height as i32 {
            // More phenotypes than the column can hold; the rest would be
            // drawn off-canvas, so they are dropped rather than overlapping.
            break;
        }
        area.draw(&Rectangle::new(
            [(swatch, y), (swatch * 2, y + swatch)],
            colour(palette[index]).filled(),
        ))
        .map_err(|e| anyhow::anyhow!("cannot draw the legend swatch: {e}"))?;

        area.draw(&Text::new(
            (*phenotype).to_string(),
            (swatch * 3, y),
            crate::canvas::label_style(theme, 14.0),
        ))
        .map_err(|e| anyhow::anyhow!("cannot draw the legend label: {e}"))?;

        y += step;
        let _ = width;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Coordinates, edges and phenotype labels of one sample.
    type Network = (Vec<[f64; 2]>, Vec<(u32, u32)>, Vec<String>);

    fn fixture() -> Network {
        let coords: Vec<[f64; 2]> = (0..12).map(|i| [i as f64, (i % 4) as f64]).collect();
        let pairs: Vec<(u32, u32)> = (0..11u32).map(|i| (i, i + 1)).collect();
        let labels: Vec<String> = (0..12).map(|i| ["A", "B"][i % 2].to_string()).collect();
        (coords, pairs, labels)
    }

    #[test]
    fn the_two_level_name_carries_both_identifiers() {
        let dir = tempfile::tempdir().unwrap();
        let (coords, pairs, labels) = fixture();
        draw(
            &Theme { dpi: 20.0 },
            &SampleId::with_sample("4", "9"),
            "patient",
            Some("sample"),
            &coords,
            &pairs,
            &labels,
            dir.path(),
        )
        .unwrap();
        assert!(dir.path().join("net_4-9.png").is_file());
    }

    #[test]
    fn an_edge_pointing_outside_the_sample_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let (coords, _, labels) = fixture();
        // A malformed edges file must not crash the whole run.
        draw(
            &Theme { dpi: 20.0 },
            &SampleId::patient_only("1"),
            "patient",
            None,
            &coords,
            &[(0, 999)],
            &labels,
            dir.path(),
        )
        .unwrap();
        assert!(dir.path().join("net_1.png").is_file());
    }

    #[test]
    fn coincident_cells_do_not_collapse_the_axes() {
        let dir = tempfile::tempdir().unwrap();
        let coords = vec![[5.0, 5.0]; 4];
        let labels = vec!["A".to_string(); 4];
        draw(
            &Theme { dpi: 20.0 },
            &SampleId::patient_only("1"),
            "patient",
            None,
            &coords,
            &[(0, 1)],
            &labels,
            dir.path(),
        )
        .unwrap();
        assert!(dir.path().join("net_1.png").is_file());
    }
}
