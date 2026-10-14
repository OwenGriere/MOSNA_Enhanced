//! Whether a directory of nodes files can be turned into spatial networks.
//!
//! The pipelines themselves check only that the three configured columns
//! exist (`assert_file_for_tysserand.py`, and its port in
//! `tysserand_network.rs`). Everything else a nodes file has to be — numeric,
//! finite coordinates, enough cells, points that are not all on one line, one
//! labelling of phenotypes across the cohort — is discovered by the
//! triangulation failing, or not discovered at all. This module checks it up
//! front, file by file and then across the cohort.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use arrow_schema::DataType;
use rayon::prelude::*;
use serde_json::{json, Value};

use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::Table;

use crate::check::naming::{scan_directory, Naming, SampleFile};
use crate::report::{Affects, Report, Severity};

/// The columns step 1 reads, and its one numeric parameter.
#[derive(Debug, Clone)]
pub struct NodesSpec {
    pub naming: Naming,
    pub x_column: String,
    pub y_column: String,
    pub phenotype_column: String,
    pub min_neighbors: usize,
}

/// What one file contributed to the cohort-level checks.
#[derive(Debug, Default)]
struct FileSummary {
    name: String,
    id: String,
    cells: usize,
    phenotype_kind: Option<&'static str>,
    phenotypes: BTreeSet<String>,
    columns: Vec<String>,
}

/// Check every nodes file of `dir`, then the cohort they form.
///
/// Returns the report and a short per-sample table.
pub fn check_nodes(dir: &Path, spec: &NodesSpec) -> (Report, Value) {
    let (files, mut report) = scan_directory(dir, &spec.naming);
    let extension = match Extension::parse(&spec.naming.extension) {
        Ok(extension) => extension,
        Err(error) => {
            report.error("bad-extension", None, error.to_string());
            return (report, json!([]));
        }
    };

    if spec.x_column == spec.y_column {
        report.error(
            "same-xy-column",
            None,
            format!(
                "X and Y are both read from `{}`: every cell would sit on the diagonal",
                spec.x_column
            ),
        );
    }

    let results: Vec<(Report, Option<FileSummary>)> = files
        .par_iter()
        .map(|file| check_file(file, extension, spec))
        .collect();

    let mut summaries = Vec::new();
    for (file_report, summary) in results {
        report.extend(file_report);
        summaries.extend(summary);
    }

    check_cohort(&summaries, spec, &mut report);

    let table: Vec<Value> = summaries
        .iter()
        .map(|s| {
            json!({
                "file": s.name,
                "id": s.id,
                "cells": s.cells,
                "phenotypes": s.phenotypes.len(),
            })
        })
        .collect();
    (report, Value::Array(table))
}

fn check_file(
    file: &SampleFile,
    extension: Extension,
    spec: &NodesSpec,
) -> (Report, Option<FileSummary>) {
    let mut report = Report::default();
    let name = file.name();
    let at = Some(name.as_str());

    let table = match read_table(&file.path, extension) {
        Ok(table) => table,
        Err(error) => {
            report.error("unreadable", at, error.to_string());
            return (report, None);
        }
    };

    let columns: Vec<String> = table
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let missing: Vec<&str> = [&spec.x_column, &spec.y_column, &spec.phenotype_column]
        .into_iter()
        .filter(|c| !table.has_column(c))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        let available: Vec<&str> = columns.iter().take(30).map(String::as_str).collect();
        report.error(
            "missing-column",
            at,
            format!(
                "missing column(s) {:?}; the file has {:?}",
                missing, available
            ),
        );
        return (report, None);
    }

    let mut summary = FileSummary {
        name: name.clone(),
        id: match &file.id.sample {
            Some(sample) => format!("{}/{sample}", file.id.patient),
            None => file.id.patient.clone(),
        },
        cells: table.n_rows(),
        columns,
        ..FileSummary::default()
    };

    if table.n_rows() == 0 {
        report.error("empty", at, "the file has no rows");
        return (report, Some(summary));
    }

    check_coordinates(&table, spec, at, &mut report);
    check_phenotypes(&table, spec, at, &mut report, &mut summary);

    if summary.columns.iter().any(|c| is_pandas_index(c)) {
        report.info(
            "pandas-index-column",
            at,
            "the file carries a pandas index column; it is read as an ordinary column",
        );
    }
    if summary
        .columns
        .iter()
        .any(|c| c == "niches" || c.starts_with("niches_"))
    {
        report.info(
            "previous-labels",
            at,
            "the file already holds niche labels from an earlier run",
        );
    }

    (report, Some(summary))
}

fn check_coordinates(table: &Table, spec: &NodesSpec, at: Option<&str>, report: &mut Report) {
    for column in [&spec.x_column, &spec.y_column] {
        if !table.is_numeric_column(column).unwrap_or(false) {
            let found = table
                .column_type(column)
                .map(|t| format!("{t:?}"))
                .unwrap_or_default();
            report.error(
                "coord-not-numeric",
                at,
                format!("coordinate column `{column}` is stored as {found}, not as numbers"),
            );
            return;
        }
    }

    let Ok(coords) = table.coords(&spec.x_column, &spec.y_column) else {
        report.error(
            "coord-unreadable",
            at,
            "the coordinates cannot be read as floats",
        );
        return;
    };

    let bad: Vec<usize> = coords
        .iter()
        .enumerate()
        .filter(|(_, p)| !p[0].is_finite() || !p[1].is_finite())
        .map(|(i, _)| i)
        .collect();
    if !bad.is_empty() {
        report.error(
            "coord-non-finite",
            at,
            format!(
                "{} cell(s) have a missing or infinite coordinate (first at row {}); \
                 the triangulation cannot place them",
                bad.len(),
                bad[0]
            ),
        );
        return;
    }

    let distinct = distinct_points(&coords);
    if distinct < 3 {
        report.error(
            "too-few-cells",
            at,
            format!("{distinct} distinct position(s): a Delaunay network needs at least 3"),
        );
        return;
    }
    if collinear(&coords) {
        report.error(
            "collinear",
            at,
            "every cell lies on one straight line: the triangulation has no triangle to build",
        );
        return;
    }

    let duplicates = coords.len() - distinct;
    if duplicates > 0 {
        report.warning(
            "duplicate-coordinates",
            at,
            format!(
                "{duplicates} cell(s) share their position with another; the triangulation \
                 keeps one of them and the others are only reconnected by the solitary-cell \
                 pass, which scipy (MOSNA_GUI) and delaunator (Rust) need not resolve alike"
            ),
        );
    }
    if coords.len() <= spec.min_neighbors {
        report.warning(
            "fewer-cells-than-min-neighbors",
            at,
            format!(
                "{} cells for `Min neighbors: {}`: no cell can reach that many neighbours",
                coords.len(),
                spec.min_neighbors
            ),
        );
    }
}

fn check_phenotypes(
    table: &Table,
    spec: &NodesSpec,
    at: Option<&str>,
    report: &mut Report,
    summary: &mut FileSummary,
) {
    let column = &spec.phenotype_column;
    summary.phenotype_kind = table.column_type(column).map(kind_of);

    let Ok(values) = table.opt_string_column(column) else {
        report.error(
            "phenotype-unreadable",
            at,
            format!("phenotype column `{column}` cannot be read as labels"),
        );
        return;
    };

    let nulls = values.iter().filter(|v| v.is_none()).count();
    if nulls > 0 {
        report.warning(
            "phenotype-null",
            at,
            format!(
                "{nulls} cell(s) have no phenotype. MOSNA_GUI drops them from the cohort's \
                 phenotypes (`dropna`) while Rust reads them as an empty label: the two \
                 implementations will not count the same categories"
            ),
        );
    }
    summary.phenotypes = values.into_iter().flatten().collect();
}

/// The checks only a whole cohort can fail.
fn check_cohort(summaries: &[FileSummary], spec: &NodesSpec, report: &mut Report) {
    if summaries.is_empty() {
        return;
    }

    // The same phenotype stored as an integer in one file and a float in
    // another reads as `3` and `3.0`: two labels in Rust, one in Python, where
    // `3 == 3.0` merges them in the set of cohort phenotypes.
    let kinds: BTreeSet<&str> = summaries.iter().filter_map(|s| s.phenotype_kind).collect();
    if kinds.contains("integer") && kinds.contains("float") {
        report.push(
            Severity::Error,
            "phenotype-type-mismatch",
            Affects::Rust,
            None,
            format!(
                "`{}` is stored as integers in some files and as floats in others: Rust \
                 reads `3` and `3.0` as two phenotypes, MOSNA_GUI as one",
                spec.phenotype_column
            ),
        );
    } else if kinds.len() > 1 {
        report.warning(
            "phenotype-type-mixed",
            None,
            format!(
                "`{}` is stored with different types across files ({:?})",
                spec.phenotype_column, kinds
            ),
        );
    }

    let union: BTreeSet<&String> = summaries.iter().flat_map(|s| &s.phenotypes).collect();
    if union.len() < 2 {
        report.warning(
            "single-phenotype",
            None,
            "the whole cohort has fewer than two phenotypes: assortativity and niche \
             composition have nothing to compare",
        );
    } else {
        report.info(
            "phenotypes",
            None,
            format!("{} phenotypes across the cohort", union.len()),
        );
    }

    for summary in summaries {
        let absent: Vec<&str> = union
            .iter()
            .filter(|p| !summary.phenotypes.contains(**p))
            .map(|p| p.as_str())
            .collect();
        if !absent.is_empty() && absent.len() * 2 >= union.len() {
            report.info(
                "phenotypes-absent",
                Some(&summary.name),
                format!(
                    "{} of the {} cohort phenotypes never occur in this sample",
                    absent.len(),
                    union.len()
                ),
            );
        }
    }

    // Columns present in some files only: harmless for steps 1 and 2, but the
    // aggregated niche analysis reads the same columns from every sample.
    let mut presence: BTreeMap<&str, usize> = BTreeMap::new();
    for summary in summaries {
        for column in &summary.columns {
            *presence.entry(column).or_default() += 1;
        }
    }
    let partial: Vec<&str> = presence
        .iter()
        .filter(|(_, n)| **n < summaries.len())
        .map(|(c, _)| *c)
        .take(10)
        .collect();
    if !partial.is_empty() {
        report.info(
            "columns-differ",
            None,
            format!("some columns are present in only part of the files: {partial:?}"),
        );
    }
}

fn kind_of(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::Dictionary(_, value) => kind_of(value),
        t if t.is_integer() => "integer",
        t if t.is_floating()
            || matches!(t, DataType::Decimal128(_, _) | DataType::Decimal256(_, _)) =>
        {
            "float"
        }
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "string",
        DataType::Boolean => "boolean",
        _ => "other",
    }
}

fn is_pandas_index(column: &str) -> bool {
    column.is_empty() || column.starts_with("Unnamed: ") || column == "__index_level_0__"
}

fn distinct_points(coords: &[[f64; 2]]) -> usize {
    // `+ 0.0` folds -0.0 into 0.0, which compare equal but differ in bits.
    let mut keys: Vec<(u64, u64)> = coords
        .iter()
        .map(|p| ((p[0] + 0.0).to_bits(), (p[1] + 0.0).to_bits()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys.len()
}

/// Whether every point lies on the line through the two first distinct ones.
fn collinear(coords: &[[f64; 2]]) -> bool {
    let origin = coords[0];
    let Some(direction) = coords
        .iter()
        .find(|p| p[0] != origin[0] || p[1] != origin[1])
        .map(|p| [p[0] - origin[0], p[1] - origin[1]])
    else {
        return true;
    };
    let scale = coords
        .iter()
        .map(|p| (p[0] - origin[0]).abs().max((p[1] - origin[1]).abs()))
        .fold(0.0, f64::max);
    let tolerance = 1e-12 * scale * scale;
    coords.iter().all(|p| {
        let cross = direction[0] * (p[1] - origin[1]) - direction[1] * (p[0] - origin[0]);
        cross.abs() <= tolerance
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collinear_points_are_detected() {
        assert!(collinear(&[[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]]));
        assert!(!collinear(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]));
    }

    #[test]
    fn duplicates_are_counted_once() {
        assert_eq!(distinct_points(&[[0.0, 0.0], [-0.0, 0.0], [1.0, 0.0]]), 2);
    }
}
