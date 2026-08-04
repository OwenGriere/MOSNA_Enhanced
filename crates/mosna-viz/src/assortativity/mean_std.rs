//! Mean assortativity across samples — port of
//! `assort_figures_mean_std_across_samples.py`.

use std::collections::BTreeSet;
use std::path::Path;

use plotters::prelude::*;

use crate::assortativity::Table;
use crate::canvas::{colour, figure, label_style, title_style};
use crate::colormap::{rd_bu_r, Gradient};
use crate::norm::SymLogNorm;
use crate::theme::{Theme, EMPTY_CELL};

/// Largest and smallest square, as fractions of a cell.
const SIZE_MAX: f64 = 0.85;
const SIZE_MIN: f64 = 0.15;

/// Draw the phenotype-by-phenotype mean assortativity.
///
/// Colour carries the mean, and the size of each square carries the standard
/// error: a small square is an uncertain estimate. Encoding uncertainty as size
/// rather than a second colour is what lets both be read at once.
pub fn draw(
    theme: &Theme,
    table: &Table,
    include_self_pairs: bool,
    save_dir: &Path,
) -> anyhow::Result<()> {
    let name = if include_self_pairs {
        "Assortativity_heatmap_across_patient.png"
    } else {
        "Assortativity_heatmap_across_patient_without_auto_paired_pheno.png"
    };
    let path = save_dir.join(name);

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

    // The phenotype vocabulary, sorted as the Python's `sorted(set(...))`.
    let mut vocabulary: BTreeSet<String> = BTreeSet::new();
    for (_, column) in &pairs {
        if let Some((left, right)) = Table::split_pair(column) {
            vocabulary.insert(left);
            vocabulary.insert(right);
        }
    }
    let phenotypes: Vec<String> = vocabulary.into_iter().collect();
    let n = phenotypes.len();

    // Figure size grows with the vocabulary: `figsize=(n * 0.7 + 5, n * 0.7 + 2)`.
    let width = n as f64 * 0.7 + 5.0;
    let height = n as f64 * 0.7 + 2.0;

    figure(&path, theme, width, height, |root| {
        let (title_area, body) = root.split_vertically(theme.font(25.0) * 2);
        title_area
            .titled(
                "Mean assortativity + std accross samples",
                title_style(theme, 25.0),
            )
            .map_err(|e| anyhow::anyhow!("cannot draw the title: {e}"))?;

        if table.is_empty() || n == 0 {
            return Ok(());
        }

        let index_of = |phenotype: &str| phenotypes.iter().position(|p| p == phenotype);

        // Mean and standard error of each pair across the samples.
        let mut mean = vec![f64::NAN; n * n];
        let mut error = vec![f64::NAN; n * n];
        for (column, name) in &pairs {
            let Some((left, right)) = Table::split_pair(name) else {
                continue;
            };
            let (Some(i), Some(j)) = (index_of(&left), index_of(&right)) else {
                continue;
            };

            let values: Vec<f64> = (0..table.rows.len())
                .map(|row| table.value(row, *column))
                .filter(|v| v.is_finite())
                .collect();
            if values.is_empty() {
                continue;
            }
            let count = values.len() as f64;
            let average = values.iter().sum::<f64>() / count;
            // Standard error of the mean, as `Series.sem()` computes it: the
            // sample standard deviation over the square root of the count.
            let sem = if values.len() > 1 {
                let variance =
                    values.iter().map(|v| (v - average).powi(2)).sum::<f64>() / (count - 1.0);
                (variance / count).sqrt()
            } else {
                0.0
            };

            for (a, b) in [(i, j), (j, i)] {
                mean[a * n + b] = average;
                error[a * n + b] = sem;
            }
        }

        let zlim = mean
            .iter()
            .filter(|v| v.is_finite())
            .fold(0.0f64, |acc, v| acc.max(v.abs()))
            .max(1e-6);
        let norm = SymLogNorm::new(SymLogNorm::threshold_for(zlim), -zlim, zlim);
        let map = rd_bu_r();

        let (error_min, error_max) = error
            .iter()
            .filter(|v| v.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(*v), hi.max(*v))
            });

        // A larger square means a smaller error, so the eye is drawn to the
        // estimates that can be trusted.
        let square_size = |sem: f64| -> f64 {
            if !sem.is_finite() {
                return 0.0;
            }
            if error_max <= error_min {
                return SIZE_MAX;
            }
            let scaled = (sem - error_min) / (error_max - error_min);
            SIZE_MAX - scaled * (SIZE_MAX - SIZE_MIN)
        };

        let label_width = (body.dim_in_pixel().0 as f64 * 0.2) as u32;
        let axis_height = theme.font(12.0) * 12;
        let (labels_area, plot_area) = body.split_horizontally(label_width);
        let (plot, axis) =
            plot_area.split_vertically(plot_area.dim_in_pixel().1.saturating_sub(axis_height));
        let (row_labels, _) =
            labels_area.split_vertically(labels_area.dim_in_pixel().1.saturating_sub(axis_height));

        let (plot_width, plot_height) = plot.dim_in_pixel();
        let cell = (plot_width as f64 / n as f64).min(plot_height as f64 / n as f64);

        for row in 0..n {
            for column in 0..n {
                let x0 = column as f64 * cell;
                let y0 = row as f64 * cell;

                // The empty background every cell sits on.
                plot.draw(&Rectangle::new(
                    [
                        (x0.round() as i32, y0.round() as i32),
                        ((x0 + cell).ceil() as i32, (y0 + cell).ceil() as i32),
                    ],
                    colour(EMPTY_CELL).filled(),
                ))
                .map_err(|e| anyhow::anyhow!("cannot draw a cell background: {e}"))?;

                let value = mean[row * n + column];
                if !value.is_finite() {
                    plot.draw(&Rectangle::new(
                        [
                            (x0.round() as i32, y0.round() as i32),
                            ((x0 + cell).ceil() as i32, (y0 + cell).ceil() as i32),
                        ],
                        colour(Gradient::BAD).filled(),
                    ))
                    .map_err(|e| anyhow::anyhow!("cannot mark a missing cell: {e}"))?;
                    continue;
                }

                let size = square_size(error[row * n + column]) * cell;
                let offset = (cell - size) / 2.0;
                plot.draw(&Rectangle::new(
                    [
                        ((x0 + offset).round() as i32, (y0 + offset).round() as i32),
                        (
                            (x0 + offset + size).round() as i32,
                            (y0 + offset + size).round() as i32,
                        ),
                    ],
                    colour(map.sample(norm.normalise(value))).filled(),
                ))
                .map_err(|e| anyhow::anyhow!("cannot draw a cell: {e}"))?;
            }
        }

        let style = label_style(theme, 12.0);
        let rotated = crate::canvas::rotated_label_style(theme, 12.0);
        for (index, phenotype) in phenotypes.iter().enumerate() {
            let position = (index as f64 * cell + cell / 2.0).round() as i32;
            row_labels
                .draw(&Text::new(phenotype.clone(), (4, position), style.clone()))
                .map_err(|e| anyhow::anyhow!("cannot draw a row label: {e}"))?;
            axis.draw(&Text::new(
                phenotype.clone(),
                (position, 4),
                rotated.clone(),
            ))
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
        let rows = (1..=3)
            .map(|i| {
                (
                    format!("patient-{i}_sample-1"),
                    vec![1.0, 4.0 + i as f64, -2.0, 3.0],
                )
            })
            .collect();
        (columns, rows)
    }

    #[test]
    fn both_variants_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);

        draw(&Theme { dpi: 12.0 }, &table, true, dir.path()).unwrap();
        draw(&Theme { dpi: 12.0 }, &table, false, dir.path()).unwrap();

        assert!(dir
            .path()
            .join("Assortativity_heatmap_across_patient.png")
            .is_file());
        assert!(dir
            .path()
            .join("Assortativity_heatmap_across_patient_without_auto_paired_pheno.png")
            .is_file());
    }

    #[test]
    fn a_single_sample_has_no_standard_error() {
        let dir = tempfile::tempdir().unwrap();
        let columns = vec!["A - B Z".to_string()];
        let rows = vec![("patient-1".to_string(), vec![2.0])];
        // One sample means the error is zero, not NaN; the figure must draw.
        draw(
            &Theme { dpi: 12.0 },
            &Table::new(&columns, &rows),
            true,
            dir.path(),
        )
        .unwrap();
        assert!(dir
            .path()
            .join("Assortativity_heatmap_across_patient.png")
            .is_file());
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
            .join("Assortativity_heatmap_across_patient.png")
            .is_file());
    }
}
