//! The mean assortativity across samples, and how sure of it we are.

use std::collections::BTreeSet;
use std::path::Path;

use mosna_core::colormap::rd_bu_r;

use crate::norm::SymLogNorm;
use crate::palette;
use crate::spec::Spec;
use crate::table::Table;

pub const KIND: &str = "assortativity_mean_std";

/// Largest and smallest square, in the units `xy` sizes a marker with.
pub const SIZE_MAX: f64 = 26.0;
pub const SIZE_MIN: f64 = 6.0;

pub fn stem(include_self_pairs: bool) -> &'static str {
    if include_self_pairs {
        "Assortativity_heatmap_across_patient"
    } else {
        "Assortativity_heatmap_across_patient_without_auto_paired_pheno"
    }
}

pub fn spec(table: &Table, include_self_pairs: bool, save_dir: &Path) -> Spec {
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

    // The phenotype vocabulary, sorted as the original's `sorted(set(...))`.
    let mut vocabulary: BTreeSet<String> = BTreeSet::new();
    for (_, column) in &pairs {
        if let Some((left, right)) = Table::split_pair(column) {
            vocabulary.insert(left);
            vocabulary.insert(right);
        }
    }
    let phenotypes: Vec<String> = vocabulary.into_iter().collect();
    let n = phenotypes.len();

    let mut mean = vec![f64::NAN; n * n];
    let mut error = vec![f64::NAN; n * n];
    for (column, name) in &pairs {
        let Some((left, right)) = Table::split_pair(name) else {
            continue;
        };
        let index_of = |phenotype: &str| phenotypes.iter().position(|p| p == phenotype);
        let (Some(i), Some(j)) = (index_of(&left), index_of(&right)) else {
            continue;
        };

        let values: Vec<f64> = (0..table.rows.len())
            .map(|row| table.value(row, *column))
            .filter(|value| value.is_finite())
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

    // A handful of enormous z-scores would flatten every other cell onto the
    // centre colour, which is why this figure alone is read logarithmically.
    let zlim = mean
        .iter()
        .filter(|value| value.is_finite())
        .fold(0.0f64, |widest, value| widest.max(value.abs()))
        .max(1e-6);
    let norm = SymLogNorm::new(SymLogNorm::threshold_for(zlim), -zlim, zlim);
    let colormap = palette::resample(
        &rd_bu_r(),
        |value| norm.normalise(value),
        -zlim,
        zlim,
        palette::STOPS,
    );

    let (error_min, error_max) = error
        .iter()
        .filter(|value| value.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), value| {
            (lo.min(*value), hi.max(*value))
        });

    // A larger square means a smaller error, so the eye is drawn to the
    // estimates that can be trusted.
    let sizes: Vec<f64> = error
        .iter()
        .map(|sem| {
            if !sem.is_finite() {
                return 0.0;
            }
            if error_max <= error_min {
                return SIZE_MAX;
            }
            let scaled = (sem - error_min) / (error_max - error_min);
            SIZE_MAX - scaled * (SIZE_MAX - SIZE_MIN)
        })
        .collect();

    Spec::new(KIND, stem(include_self_pairs), save_dir)
        .set("title", "Mean assortativity + std accross samples")
        .set("labels", serde_json::json!(phenotypes))
        .set("colormap", serde_json::json!(colormap))
        .set("domain", serde_json::json!([-zlim, zlim]))
        // The figure grows with the vocabulary: `figsize=(n * 0.7 + 5, ...)`.
        .set("width", ((n as f64 * 0.7 + 5.0) * 100.0).round() as i64)
        .set("height", ((n as f64 * 0.7 + 2.0) * 100.0).round() as i64)
        .set_f64_blob("z", &mean, &[n, n])
        .set_f64_blob("sizes", &sizes, &[n, n])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
        (
            vec!["A - A Z".into(), "A - B Z".into(), "B - B Z".into()],
            vec![
                ("patient-1_sample-1".to_string(), vec![1.0, 2.0, 3.0]),
                ("patient-2_sample-1".to_string(), vec![3.0, 4.0, 5.0]),
            ],
        )
    }

    #[test]
    fn the_grid_is_the_phenotype_vocabulary_sorted() {
        let (columns, rows) = table();
        let json = spec(&Table::new(&columns, &rows), true, Path::new("/out")).to_json();

        assert_eq!(json["labels"], serde_json::json!(["A", "B"]));
        assert_eq!(json["z"]["shape"], serde_json::json!([2, 2]));
        assert_eq!(json["sizes"]["shape"], serde_json::json!([2, 2]));
    }

    /// The matrix is symmetric: a pair is the same measurement read from
    /// either phenotype.
    #[test]
    fn a_pair_appears_on_both_sides_of_the_diagonal() {
        let (columns, rows) = table();
        let spec = spec(&Table::new(&columns, &rows), true, Path::new("/out"));
        let z = spec.blob_values("z");

        // A - B averages 2 and 4.
        assert!((z[1] - 3.0).abs() < 1e-12, "{z:?}");
        assert!((z[2] - 3.0).abs() < 1e-12, "{z:?}");
    }

    /// A larger square means a smaller error, so the eye is drawn to the
    /// estimates that can be trusted.
    #[test]
    fn the_square_shrinks_as_the_uncertainty_grows() {
        let columns = vec!["A - A Z".to_string(), "A - B Z".to_string()];
        let rows = vec![
            ("p-1_s-1".to_string(), vec![1.0, 0.0]),
            ("p-2_s-1".to_string(), vec![1.0, 8.0]),
        ];
        let spec = spec(&Table::new(&columns, &rows), true, Path::new("/out"));
        let sizes = spec.blob_values("sizes");

        // A - A is measured identically twice: no error at all, biggest square.
        assert!((sizes[0] - SIZE_MAX).abs() < 1e-9, "{sizes:?}");
        // A - B swings from 0 to 8: the largest error, smallest square.
        assert!((sizes[1] - SIZE_MIN).abs() < 1e-9, "{sizes:?}");
    }

    /// A pair nobody measured has no mean to colour and no error to size.
    #[test]
    fn an_unmeasured_pair_is_left_out_rather_than_drawn_at_zero() {
        let columns = vec!["A - A Z".to_string(), "A - B Z".to_string()];
        let rows = vec![("p-1_s-1".to_string(), vec![1.0, f64::NAN])];
        let spec = spec(&Table::new(&columns, &rows), true, Path::new("/out"));

        assert!(spec.blob_values("z")[1].is_nan());
    }

    #[test]
    fn the_variant_without_self_pairs_has_its_own_file() {
        let (columns, rows) = table();
        let table = Table::new(&columns, &rows);
        assert_eq!(
            spec(&table, true, Path::new("/out")).stem(),
            "Assortativity_heatmap_across_patient"
        );
        assert!(spec(&table, false, Path::new("/out"))
            .stem()
            .ends_with("_without_auto_paired_pheno"));
    }
}
