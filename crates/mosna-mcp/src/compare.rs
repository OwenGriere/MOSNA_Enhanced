//! Comparing what two runs produced — typically one per implementation.
//!
//! Networks are compared exactly: Delaunay is deterministic, so two runs on the
//! same nodes must give the same edges, and an edge Jaccard below one is a bug
//! rather than noise.
//!
//! Partitions are compared by agreement indices, and the result carries how to
//! read them. MOSNA_GUI's UMAP is unseeded (`random_state=None`): two of its
//! runs on identical inputs agree at an ARI of about 0.5, so an ARI between
//! Rust and Python can never be expected above Python's agreement with itself.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde_json::{json, Value};

use crate::agreement::{adjusted_mutual_information, adjusted_rand_index};
use mosna_io::read::get_opener::{read_table, read_table_columns, Extension};
use mosna_io::SampleId;

use crate::check::naming::{scan_directory, Naming, SampleFile};

fn index(dir: &Path, naming: &Naming) -> anyhow::Result<BTreeMap<SampleId, SampleFile>> {
    let (files, report) = scan_directory(dir, naming);
    if files.is_empty() {
        let reason = report
            .findings
            .first()
            .map(|f| f.message.clone())
            .unwrap_or_default();
        anyhow::bail!("no network in {}: {reason}", dir.display());
    }
    Ok(files.into_iter().map(|f| (f.id.clone(), f)).collect())
}

fn label(id: &SampleId) -> String {
    match &id.sample {
        Some(sample) => format!("{}/{sample}", id.patient),
        None => id.patient.clone(),
    }
}

fn edge_set(nodes: &Path, extension: Extension) -> anyhow::Result<BTreeSet<(u32, u32)>> {
    let name = nodes
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let edges = nodes.with_file_name(format!(
        "edges_{}",
        name.strip_prefix("nodes_").unwrap_or(name)
    ));
    let pairs = read_table(&edges, extension)?.edges()?;
    Ok(pairs
        .into_iter()
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect())
}

/// Edge-for-edge comparison of two network directories, sample by sample.
pub fn compare_networks(a: (&Path, &Naming), b: (&Path, &Naming)) -> anyhow::Result<Value> {
    let (left, right) = (index(a.0, a.1)?, index(b.0, b.1)?);
    let (ext_a, ext_b) = (
        Extension::parse(&a.1.extension)?,
        Extension::parse(&b.1.extension)?,
    );

    let mut rows = Vec::new();
    for (id, file_a) in &left {
        let Some(file_b) = right.get(id) else {
            continue;
        };
        let cells_a = read_table(&file_a.path, ext_a)?.n_rows();
        let cells_b = read_table(&file_b.path, ext_b)?.n_rows();
        if cells_a != cells_b {
            rows.push(json!({
                "sample": label(id),
                "cells": [cells_a, cells_b],
                "jaccard": null,
                "note": "different cell counts: edges are row positions and cannot be compared",
            }));
            continue;
        }
        let edges_a = edge_set(&file_a.path, ext_a)?;
        let edges_b = edge_set(&file_b.path, ext_b)?;
        let common = edges_a.intersection(&edges_b).count();
        let union = edges_a.len() + edges_b.len() - common;
        let jaccard = if union == 0 {
            1.0
        } else {
            common as f64 / union as f64
        };
        rows.push(json!({
            "sample": label(id),
            "cells": cells_a,
            "edges": [edges_a.len(), edges_b.len()],
            "only_in_a": edges_a.len() - common,
            "only_in_b": edges_b.len() - common,
            "jaccard": jaccard,
        }));
    }

    let jaccards: Vec<f64> = rows.iter().filter_map(|r| r["jaccard"].as_f64()).collect();
    let identical = jaccards.iter().filter(|j| **j == 1.0).count();
    let min = jaccards.iter().copied().fold(f64::INFINITY, f64::min);
    rows.sort_by(|x, y| {
        let (x, y) = (
            x["jaccard"].as_f64().unwrap_or(-1.0),
            y["jaccard"].as_f64().unwrap_or(-1.0),
        );
        x.total_cmp(&y)
    });
    rows.truncate(30);

    Ok(json!({
        "samples_compared": jaccards.len(),
        "only_in_a": left.keys().filter(|k| !right.contains_key(*k)).map(label).collect::<Vec<_>>(),
        "only_in_b": right.keys().filter(|k| !left.contains_key(*k)).map(label).collect::<Vec<_>>(),
        "identical": identical,
        "min_jaccard": if jaccards.is_empty() { Value::Null } else { json!(min) },
        "mean_jaccard": if jaccards.is_empty() { Value::Null } else {
            json!(jaccards.iter().sum::<f64>() / jaccards.len() as f64)
        },
        "worst_samples": rows,
        "how_to_read": "Delaunay is deterministic: on the same nodes files the expected Jaccard \
                        is exactly 1. Anything lower points at the inputs (duplicate positions, \
                        different rows) or at a porting bug, not at randomness.",
    }))
}

/// Read one label column across a cohort, in sample order.
fn labels(
    files: &BTreeMap<SampleId, SampleFile>,
    ids: &[&SampleId],
    extension: Extension,
    column: &str,
) -> anyhow::Result<Vec<Vec<String>>> {
    ids.iter()
        .map(|id| {
            let file = &files[*id];
            // A parquet projection on a missing column fails outright, before
            // the presence test below could list what is there instead.
            let table = read_table_columns(&file.path, extension, &[column]).ok();
            let Some(table) = table.filter(|t| t.has_column(column)) else {
                let all = read_table(&file.path, extension)?;
                let candidates: Vec<&str> = all
                    .column_names()
                    .into_iter()
                    .filter(|c| c.starts_with("niches"))
                    .collect();
                anyhow::bail!(
                    "no column `{column}` in {}; label columns there: {candidates:?}",
                    file.name()
                );
            };
            Ok(table.string_column(column)?)
        })
        .collect()
}

fn encode(values: &[String], codes: &mut HashMap<String, u32>) -> Vec<u32> {
    values
        .iter()
        .map(|v| {
            let next = codes.len() as u32;
            *codes.entry(v.clone()).or_insert(next)
        })
        .collect()
}

/// Agreement between two niche partitions of the same cells.
pub fn compare_partitions(
    a: (&Path, &Naming, &str),
    b: (&Path, &Naming, &str),
    per_sample: bool,
) -> anyhow::Result<Value> {
    let (left, right) = (index(a.0, a.1)?, index(b.0, b.1)?);
    let ids: Vec<&SampleId> = left.keys().filter(|id| right.contains_key(*id)).collect();
    if ids.is_empty() {
        anyhow::bail!("the two directories have no sample in common");
    }

    let labels_a = labels(&left, &ids, Extension::parse(&a.1.extension)?, a.2)?;
    let labels_b = labels(&right, &ids, Extension::parse(&b.1.extension)?, b.2)?;

    let (mut codes_a, mut codes_b) = (HashMap::new(), HashMap::new());
    let (mut all_a, mut all_b) = (Vec::new(), Vec::new());
    let mut samples = Vec::new();
    for ((id, la), lb) in ids.iter().zip(&labels_a).zip(&labels_b) {
        if la.len() != lb.len() {
            anyhow::bail!(
                "sample {} has {} cells in A and {} in B: not the same cells",
                label(id),
                la.len(),
                lb.len()
            );
        }
        let (ca, cb) = (encode(la, &mut codes_a), encode(lb, &mut codes_b));
        if per_sample {
            samples.push(json!({
                "sample": label(id),
                "cells": ca.len(),
                "ari": adjusted_rand_index(&ca, &cb)?,
            }));
        }
        all_a.extend(ca);
        all_b.extend(cb);
    }

    let mut result = json!({
        "cells": all_a.len(),
        "samples": ids.len(),
        "niches": [codes_a.len(), codes_b.len()],
        "ari": adjusted_rand_index(&all_a, &all_b)?,
        "ami": adjusted_mutual_information(&all_a, &all_b)?,
        "how_to_read": "1 means the same partition up to renaming, about 0 means unrelated. \
                        MOSNA_GUI is unseeded (UMAP random_state=None): two of its own runs on \
                        the same data agreed at ARI ≈ 0.51 on the IMC parity study, so an ARI \
                        against a Python run is bounded by that noise floor and cannot measure \
                        porting fidelity on its own. Compare Rust with Rust, or use \
                        transformations whose answer is known (isometries → 1, shuffled \
                        phenotypes → 0).",
    });
    if per_sample {
        result["per_sample"] = Value::Array(samples);
    }
    Ok(result)
}
