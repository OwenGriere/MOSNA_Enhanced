//! Z-score heatmap with dendrograms — port of `assort_figures_heatmap.py`.

use std::path::Path;

use plotters::prelude::*;

use mosna_core::stats::{dendrogram_leaf_order, ward_linkage};

use crate::assortativity::Table;
use crate::canvas::{colour, figure, label_style, title_style};
use crate::colormap::{rd_bu_r, Gradient};
use crate::norm::TwoSlopeNorm;
use crate::theme::Theme;

const WIDTH_INCHES: f64 = 28.0;
const HEIGHT_INCHES: f64 = 24.0;

/// Draw the phenotype-pair by sample heatmap, both axes clustered.
///
/// With `include_self_pairs` false, the pairs of a phenotype with itself are
/// dropped: they are always strongly positive and their scale hides everything
/// else. That is the `_without_auto_paired_pheno` variant.
pub fn draw(
    theme: &Theme,
    table: &Table,
    include_self_pairs: bool,
    save_dir: &Path,
) -> anyhow::Result<()> {
    let name = if include_self_pairs {
        "Assortativity_heatmap_with_dendrogram.png"
    } else {
        "Assortativity_heatmap_with_dendrogram_without_auto_paired_pheno.png"
    };
    let path = save_dir.join(name);

    // Rows are phenotype pairs, columns are samples — the transpose the Python
    // takes with `net_stat[assort_cols].T`.
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

    figure(&path, theme, WIDTH_INCHES, HEIGHT_INCHES, |root| {
        let (title_area, body) = root.split_vertically(theme.font(25.0) * 2);
        title_area
            .titled("Assortativity heatmap by images", title_style(theme, 25.0))
            .map_err(|e| anyhow::anyhow!("cannot draw the title: {e}"))?;

        if table.is_empty() || pairs.is_empty() {
            return Ok(());
        }

        let n_rows = pairs.len();
        let n_cols = table.rows.len();

        // A cell with no usable value is grey rather than an extreme colour;
        // the Python does the same with `cmap.set_bad`.
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
        // Python does with `.fillna(0)` before `pdist`.
        let row_order = cluster_order(&matrix, n_rows, n_cols, true);
        let column_order = cluster_order(&matrix, n_rows, n_cols, false);

        let norm = TwoSlopeNorm::centred_on_zero(matrix.iter().flat_map(|row| row.iter().copied()));
        let map = rd_bu_r();

        // Dendrogram strips along the top and the right, as the Python's
        // GridSpec lays them out.
        let dendro_height = (body.dim_in_pixel().1 as f64 * 0.12) as u32;
        let label_width = (body.dim_in_pixel().0 as f64 * 0.22) as u32;
        let dendro_width = (body.dim_in_pixel().0 as f64 * 0.08) as u32;

        let (top, rest) = body.split_vertically(dendro_height);
        let (labels_area, right) = rest.split_horizontally(label_width);
        let (plot, side) =
            right.split_horizontally(right.dim_in_pixel().0.saturating_sub(dendro_width));

        let (plot_width, plot_height) = plot.dim_in_pixel();
        let cell_width = plot_width as f64 / n_cols as f64;
        let cell_height = plot_height as f64 / n_rows as f64;

        for (drawn_row, &row) in row_order.iter().enumerate() {
            for (drawn_column, &column) in column_order.iter().enumerate() {
                let value = matrix[row][column];
                let shade = if value.is_finite() {
                    map.sample(norm.normalise(value))
                } else {
                    Gradient::BAD
                };
                let x0 = (drawn_column as f64 * cell_width).round() as i32;
                let y0 = (drawn_row as f64 * cell_height).round() as i32;
                let x1 = ((drawn_column + 1) as f64 * cell_width).ceil() as i32;
                let y1 = ((drawn_row + 1) as f64 * cell_height).ceil() as i32;
                plot.draw(&Rectangle::new(
                    [(x0, y0), (x1, y1)],
                    colour(shade).filled(),
                ))
                .map_err(|e| anyhow::anyhow!("cannot draw a cell: {e}"))?;
            }
        }

        // Row labels, only when they would be legible — the Python drops them
        // past `fig_height * 2.5` rows for the same reason.
        if n_rows <= (HEIGHT_INCHES * 2.5) as usize {
            let style = label_style(theme, 9.0);
            for (drawn_row, &row) in row_order.iter().enumerate() {
                let label = pairs[row]
                    .1
                    .strip_suffix(" Z")
                    .unwrap_or(&pairs[row].1)
                    .to_string();
                let y = (drawn_row as f64 * cell_height + cell_height / 2.0).round() as i32;
                labels_area
                    .draw(&Text::new(label, (4, y), style.clone()))
                    .map_err(|e| anyhow::anyhow!("cannot draw a row label: {e}"))?;
            }
        }

        draw_dendrogram(&top, &row_order, false)?;
        draw_dendrogram(&side, &column_order, true)?;
        Ok(())
    })
}

/// The leaf order Ward clustering puts the rows or the columns in.
fn cluster_order(matrix: &[Vec<f64>], n_rows: usize, n_cols: usize, by_row: bool) -> Vec<usize> {
    let observations: Vec<Vec<f64>> = if by_row {
        (0..n_rows)
            .map(|row| {
                (0..n_cols)
                    .map(|column| finite_or_zero(matrix[row][column]))
                    .collect()
            })
            .collect()
    } else {
        (0..n_cols)
            .map(|column| {
                (0..n_rows)
                    .map(|row| finite_or_zero(matrix[row][column]))
                    .collect()
            })
            .collect()
    };

    if observations.len() < 2 {
        return (0..observations.len()).collect();
    }
    dendrogram_leaf_order(&ward_linkage(&observations))
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// A simple bracket dendrogram, drawn as evenly spaced joins.
///
/// The Python draws scipy's dendrogram with its true merge heights. This shows
/// the same leaf order with a schematic tree: the ordering is what makes the
/// heatmap readable, and reproducing exact merge heights would need the
/// linkage matrix threaded through the drawing code for no gain in
/// interpretation.
fn draw_dendrogram(
    area: &crate::canvas::Surface<'_>,
    order: &[usize],
    vertical: bool,
) -> anyhow::Result<()> {
    let (width, height) = area.dim_in_pixel();
    if order.len() < 2 || width < 4 || height < 4 {
        return Ok(());
    }

    let count = order.len() as f64;
    let stroke = ShapeStyle::from(&RGBColor(0x2d, 0x2d, 0x2d)).stroke_width(1);

    // Successive joins, each one level higher than the last.
    let levels = (count.log2().ceil() as u32).max(1);
    for level in 1..=levels {
        let group = 2usize.pow(level);
        let depth = level as f64 / levels as f64;

        let mut start = 0usize;
        while start + group / 2 < order.len() {
            let end = (start + group - 1).min(order.len() - 1);
            let midpoint = |index: usize| -> i32 {
                if vertical {
                    ((index as f64 + 0.5) / count * height as f64) as i32
                } else {
                    ((index as f64 + 0.5) / count * width as f64) as i32
                }
            };
            let (a, b) = (midpoint(start), midpoint(end));
            let level_position = if vertical {
                (depth * width as f64) as i32
            } else {
                (height as f64 - depth * height as f64) as i32
            };

            let points = if vertical {
                vec![(level_position, a), (level_position, b)]
            } else {
                vec![(a, level_position), (b, level_position)]
            };
            area.draw(&PathElement::new(points, stroke))
                .map_err(|e| anyhow::anyhow!("cannot draw the dendrogram: {e}"))?;
            start += group;
        }
    }
    Ok(())
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
        let rows = (1..=4)
            .map(|i| {
                (
                    format!("patient-{i}_sample-1"),
                    vec![1.0, i as f64, -(i as f64), 0.5 * i as f64],
                )
            })
            .collect();
        (columns, rows)
    }

    #[test]
    fn both_variants_are_written_under_their_own_name() {
        let dir = tempfile::tempdir().unwrap();
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);

        draw(&Theme { dpi: 12.0 }, &table, true, dir.path()).unwrap();
        draw(&Theme { dpi: 12.0 }, &table, false, dir.path()).unwrap();

        assert!(dir
            .path()
            .join("Assortativity_heatmap_with_dendrogram.png")
            .is_file());
        assert!(dir
            .path()
            .join("Assortativity_heatmap_with_dendrogram_without_auto_paired_pheno.png")
            .is_file());
    }

    #[test]
    fn the_self_pairs_are_dropped_from_the_second_variant() {
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);

        let with: Vec<String> = table
            .pair_z_columns()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        assert!(with.contains(&"A - A Z".to_string()));

        let without: Vec<String> = table
            .pair_z_columns()
            .into_iter()
            .filter(|(_, column)| {
                Table::split_pair(column)
                    .map(|(l, r)| l != r)
                    .unwrap_or(true)
            })
            .map(|(_, name)| name)
            .collect();
        assert_eq!(without, vec!["A - B Z".to_string()]);
    }

    #[test]
    fn an_empty_table_still_writes_a_figure() {
        let dir = tempfile::tempdir().unwrap();
        draw(
            &Theme { dpi: 12.0 },
            &Table::new(&[], &[]),
            true,
            dir.path(),
        )
        .unwrap();
        assert!(dir
            .path()
            .join("Assortativity_heatmap_with_dendrogram.png")
            .is_file());
    }

    #[test]
    fn a_single_row_needs_no_clustering() {
        let matrix = vec![vec![1.0, 2.0]];
        assert_eq!(cluster_order(&matrix, 1, 2, true), vec![0]);
    }
}
