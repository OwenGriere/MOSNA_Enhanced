//! Whether a directory holds networks steps 2 and 3 can read.
//!
//! A network is a `nodes_*` file and an `edges_*` file of the same name, the
//! edges being row positions in the nodes file — not identifiers. Steps 2 and
//! 3 accept a directory of them from anywhere (`Network directory`), so it may
//! not come from step 1 at all; nothing on either side checks that an edge
//! points at a row that exists until an index goes out of bounds deep inside
//! the assortativity.

use std::collections::BTreeSet;
use std::path::Path;

use rayon::prelude::*;
use serde_json::{json, Value};

use mosna_io::read::get_opener::{read_table, table_rows, Extension};

use crate::check::naming::{file_name, scan_directory, Naming, SampleFile};
use crate::report::Report;

/// Check every network of `dir`.
///
/// `phenotype_column`, when given, must be present in every nodes file: it is
/// the column step 2 (and step 3's composition) reads.
pub fn check_network(
    dir: &Path,
    naming: &Naming,
    phenotype_column: Option<&str>,
) -> (Report, Value) {
    let (files, mut report) = scan_directory(dir, naming);
    let extension = match Extension::parse(&naming.extension) {
        Ok(extension) => extension,
        Err(error) => {
            report.error("bad-extension", None, error.to_string());
            return (report, json!([]));
        }
    };

    let results: Vec<(Report, Option<Value>)> = files
        .par_iter()
        .map(|file| check_one(file, extension, phenotype_column))
        .collect();

    let mut table = Vec::new();
    for (file_report, row) in results {
        report.extend(file_report);
        table.extend(row);
    }

    // Edges files with no nodes file are skipped without a word by both sides.
    let expected: BTreeSet<String> = files.iter().map(|f| edges_name(&f.name())).collect();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let suffix = format!(".{}", naming.extension);
        for entry in entries.filter_map(|e| e.ok()) {
            let name = file_name(&entry.path());
            if name.starts_with("edges_") && name.ends_with(&suffix) && !expected.contains(&name) {
                report.warning(
                    "orphan-edges",
                    Some(&name),
                    "edges file without a matching nodes file: it is ignored",
                );
            }
        }
    }

    (report, Value::Array(table))
}

fn edges_name(nodes_name: &str) -> String {
    format!(
        "edges_{}",
        nodes_name.strip_prefix("nodes_").unwrap_or(nodes_name)
    )
}

fn check_one(
    file: &SampleFile,
    extension: Extension,
    phenotype_column: Option<&str>,
) -> (Report, Option<Value>) {
    let mut report = Report::default();
    let name = file.name();
    let at = Some(name.as_str());

    let cells = match phenotype_column {
        Some(column) => match read_table(&file.path, extension) {
            Ok(table) => {
                if !table.has_column(column) {
                    report.error(
                        "missing-column",
                        at,
                        format!("no `{column}` column, which the analysis reads as phenotype"),
                    );
                } else if let Ok(values) = table.opt_string_column(column) {
                    let nulls = values.iter().filter(|v| v.is_none()).count();
                    if nulls > 0 {
                        report.warning(
                            "phenotype-null",
                            at,
                            format!(
                                "{nulls} cell(s) have no phenotype; MOSNA_GUI drops them, \
                                 Rust reads an empty label"
                            ),
                        );
                    }
                }
                table.n_rows()
            }
            Err(error) => {
                report.error("unreadable", at, error.to_string());
                return (report, None);
            }
        },
        None => match table_rows(&file.path, extension) {
            Ok(rows) => rows,
            Err(error) => {
                report.error("unreadable", at, error.to_string());
                return (report, None);
            }
        },
    };

    let edges_path = file.path.with_file_name(edges_name(&name));
    if !edges_path.is_file() {
        report.error(
            "edges-missing",
            at,
            format!("no `{}` next to it", file_name(&edges_path)),
        );
        return (report, None);
    }

    let edges = match read_table(&edges_path, extension).and_then(|t| t.edges()) {
        Ok(edges) => edges,
        Err(error) => {
            report.error(
                "edges-malformed",
                Some(&file_name(&edges_path)),
                error.to_string(),
            );
            return (report, None);
        }
    };

    let stats = EdgeStats::of(&edges, cells);
    if stats.out_of_range > 0 {
        report.error(
            "edge-out-of-range",
            at,
            format!(
                "{} edge(s) point past the {cells} rows of the nodes file: edges are row \
                 positions, so this network does not belong to these nodes",
                stats.out_of_range
            ),
        );
    }
    if stats.self_loops > 0 {
        report.warning(
            "self-loops",
            at,
            format!("{} edge(s) join a cell to itself", stats.self_loops),
        );
    }
    if stats.duplicates > 0 {
        report.warning(
            "duplicate-edges",
            at,
            format!(
                "{} edge(s) repeat another, possibly in the other direction",
                stats.duplicates
            ),
        );
    }
    if stats.isolated > 0 {
        report.info(
            "isolated-cells",
            at,
            format!("{} cell(s) have no neighbour", stats.isolated),
        );
    }

    let row = json!({
        "file": name,
        "cells": cells,
        "edges": stats.unique,
        "mean_degree": if cells > 0 { 2.0 * stats.unique as f64 / cells as f64 } else { 0.0 },
        "isolated": stats.isolated,
    });
    (report, Some(row))
}

#[derive(Debug, Default, PartialEq)]
struct EdgeStats {
    unique: usize,
    duplicates: usize,
    self_loops: usize,
    out_of_range: usize,
    isolated: usize,
}

impl EdgeStats {
    fn of(edges: &[(u32, u32)], cells: usize) -> Self {
        let mut stats = EdgeStats::default();
        let mut seen = BTreeSet::new();
        let mut degree = vec![0usize; cells];
        for &(a, b) in edges {
            if a as usize >= cells || b as usize >= cells {
                stats.out_of_range += 1;
                continue;
            }
            if a == b {
                stats.self_loops += 1;
                continue;
            }
            if seen.insert((a.min(b), a.max(b))) {
                degree[a as usize] += 1;
                degree[b as usize] += 1;
            } else {
                stats.duplicates += 1;
            }
        }
        stats.unique = seen.len();
        stats.isolated = degree.iter().filter(|d| **d == 0).count();
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_defects_are_counted_separately() {
        let stats = EdgeStats::of(&[(0, 1), (1, 0), (2, 2), (0, 9)], 4);
        assert_eq!(
            stats,
            EdgeStats {
                unique: 1,
                duplicates: 1,
                self_loops: 1,
                out_of_range: 1,
                isolated: 2,
            }
        );
    }

    #[test]
    fn the_edges_file_is_named_after_the_nodes_file() {
        assert_eq!(
            edges_name("nodes_patient-1_sample-2.parquet"),
            "edges_patient-1_sample-2.parquet"
        );
    }
}
