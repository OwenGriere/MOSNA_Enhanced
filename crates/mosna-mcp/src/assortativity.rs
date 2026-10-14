//! Comparing two `net_stat.csv`, with a tolerance that knows where the noise is.
//!
//! Both implementations seed their permutations (`np.random.seed(0)` on one
//! side, a fixed base seed on the other) but not with the same generator, so
//! the null distributions are drawn from different streams. A single tolerance
//! for the whole table would therefore be wrong twice over: too loose for the
//! columns that involve no randomness, too tight for the ones that do.
//!
//! The columns fall in four kinds, by the suffix the pipeline gives them:
//!
//! | kind  | columns                                   | expected difference |
//! |-------|-------------------------------------------|---------------------|
//! | exact | `# total`, `% <phenotype>`, `assort`, `RAW` | none: same network, same counts |
//! | mean  | `… MEAN`                                  | Monte-Carlo error of a mean of `n` draws |
//! | std   | `… STD`                                   | Monte-Carlo error of a standard deviation |
//! | z     | `… Z`                                     | both of the above, propagated |
//!
//! For one run of `n` permutations, with σ the spread of the null:
//!
//! ```text
//! se(MEAN) = σ / √n        se(STD) ≈ σ / √(2(n−1))        se(Z) ≈ √((1 + z²/2) / n)
//! ```
//!
//! The two runs are independent, so the variances add, and a cell agrees when
//! the difference is within `sigmas` standard errors. The approximations assume
//! a roughly normal null, which a pair of rare phenotypes (a handful of edges)
//! does not have: outliers concentrated on rare pairs are expected, outliers on
//! an exact column never are.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use mosna_io::read::get_opener::{read_table, Extension};

/// How a column of `net_stat.csv` came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Exact,
    Mean,
    Std,
    Z,
}

impl Kind {
    /// Classify a column by its name, as `sample_assort_mixmat` builds it.
    pub fn of(column: &str) -> (Kind, &str) {
        // The abundance columns first: a phenotype may well be called `Z`.
        if column == "# total" || column.starts_with("% ") {
            return (Kind::Exact, column);
        }
        for (suffix, kind) in [(" MEAN", Kind::Mean), (" STD", Kind::Std), (" Z", Kind::Z)] {
            if let Some(base) = column.strip_suffix(suffix) {
                return (kind, base);
            }
        }
        (Kind::Exact, column)
    }

    fn as_str(self) -> &'static str {
        match self {
            Kind::Exact => "exact",
            Kind::Mean => "mean",
            Kind::Std => "std",
            Kind::Z => "z",
        }
    }
}

/// What "the same" means for this comparison.
#[derive(Debug, Clone)]
pub struct Tolerance {
    /// Standard errors a statistical column may differ by.
    pub sigmas: f64,
    /// Permutations behind each table.
    pub n_shuffle: (usize, usize),
    /// For exact columns: `|a − b| ≤ atol + rtol · max(|a|, |b|)`, which only
    /// absorbs how each side prints its floats.
    pub exact_rtol: f64,
    pub exact_atol: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            sigmas: 4.0,
            n_shuffle: (0, 0),
            exact_rtol: 1e-9,
            exact_atol: 1e-12,
        }
    }
}

/// `net_stat.csv` itself, or the working directory that holds it.
pub fn resolve_table(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("Assortativity").join("net_stat.csv")
    } else {
        path.to_path_buf()
    }
}

/// One table: sample id → column → value.
struct NetStat {
    columns: Vec<String>,
    rows: BTreeMap<String, BTreeMap<String, f64>>,
}

fn read_net_stat(path: &Path) -> anyhow::Result<NetStat> {
    let table = read_table(path, Extension::Csv)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let ids = table.string_column("id").map_err(|_| {
        anyhow::anyhow!("{} has no `id` column: not a net_stat.csv", path.display())
    })?;
    let columns: Vec<String> = table
        .column_names()
        .into_iter()
        .filter(|c| *c != "id")
        .map(str::to_string)
        .collect();

    let mut rows: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for column in &columns {
        // Through text rather than a float cast: pandas leaves a NaN cell empty
        // and writes `inf`, and Rust's parser reads both the way they were meant.
        let values = table.opt_string_column(column)?;
        for (id, value) in ids.iter().zip(values) {
            let number = value
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<f64>().unwrap_or(f64::NAN))
                .unwrap_or(f64::NAN);
            rows.entry(id.clone())
                .or_default()
                .insert(column.clone(), number);
        }
    }
    Ok(NetStat { columns, rows })
}

/// Two-sided tail of the standard normal beyond `k`: the share of cells a
/// `k`-sigma test flags in two runs that agree perfectly in law.
fn two_sided_tail(k: f64) -> f64 {
    // erfc by Abramowitz & Stegun 7.1.26, |error| < 1.5e-7: ample for an
    // expected count of false alarms.
    let x = k / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    poly * (-x * x).exp()
}

#[derive(Default)]
struct KindSummary {
    cells: usize,
    outside: usize,
    /// Cells whose null is too degenerate for a normal error to mean anything.
    unreliable: usize,
    max_abs_diff: f64,
}

/// Below this ratio of the null's mean to its spread, a pair is too rare for
/// the normal approximation.
///
/// For a count that is roughly Poisson with λ edges expected per
/// permutation, mean / spread = √λ: under one, most permutations draw no edge
/// at all, and the null is a spike at zero with an occasional hit. Two such
/// nulls differ by whether 1 or 3 permutations of 200 happened to hit, which
/// moves a z-score by whole units. On a real Rust/Python comparison (4 samples,
/// 200 permutations) every cell outside tolerance had a ratio below 0.7, and
/// none of the 723 above 1 did.
const MIN_MEAN_OVER_SPREAD: f64 = 1.0;

/// Whether the permutation statistics of `base` can be compared at all, and
/// if not, why.
fn unreliable(
    row_a: &BTreeMap<String, f64>,
    row_b: &BTreeMap<String, f64>,
    base: &str,
) -> Option<&'static str> {
    for row in [row_a, row_b] {
        let spread = row.get(&format!("{base} STD")).copied().unwrap_or(f64::NAN);
        if !spread.is_finite() || spread <= 0.0 {
            return Some("degenerate null: every permutation gave the same value");
        }
        // The assortativity coefficient is centred on zero under the null, so
        // its mean says nothing about rarity; the pairs are counts.
        if base != "assort" {
            let mean = row
                .get(&format!("{base} MEAN"))
                .copied()
                .unwrap_or(f64::NAN);
            let ratio = mean / spread;
            if ratio.is_nan() || ratio < MIN_MEAN_OVER_SPREAD {
                return Some("rare pair: under one edge expected per permutation");
            }
        }
    }
    None
}

/// Compare two assortativity tables.
pub fn compare_assortativity(a: &Path, b: &Path, tolerance: &Tolerance) -> anyhow::Result<Value> {
    let (path_a, path_b) = (resolve_table(a), resolve_table(b));
    let (left, right) = (read_net_stat(&path_a)?, read_net_stat(&path_b)?);
    let (na, nb) = tolerance.n_shuffle;
    if na < 2 || nb < 2 {
        anyhow::bail!("the tolerance needs the number of permutations of each run (at least 2)");
    }
    let (inv_n, inv_n1) = (
        1.0 / na as f64 + 1.0 / nb as f64,
        1.0 / (2.0 * (na - 1) as f64) + 1.0 / (2.0 * (nb - 1) as f64),
    );

    let columns_a: BTreeSet<&String> = left.columns.iter().collect();
    let columns_b: BTreeSet<&String> = right.columns.iter().collect();
    let ids: Vec<&String> = left
        .rows
        .keys()
        .filter(|id| right.rows.contains_key(*id))
        .collect();
    if ids.is_empty() {
        anyhow::bail!(
            "no sample id in common between {} and {}",
            path_a.display(),
            path_b.display()
        );
    }

    let mut summaries: BTreeMap<Kind, KindSummary> = BTreeMap::new();
    let mut worst: Vec<(f64, Value)> = Vec::new();
    let mut statistical_cells = 0usize;
    let mut statistical_outside = 0usize;

    for id in &ids {
        let (row_a, row_b) = (&left.rows[*id], &right.rows[*id]);
        for column in left.columns.iter().filter(|c| columns_b.contains(c)) {
            let (kind, base) = Kind::of(column);
            let (va, vb) = (row_a[column], row_b[column]);
            let summary = summaries.entry(kind).or_default();
            summary.cells += 1;

            if kind != Kind::Exact && unreliable(row_a, row_b, base).is_some() {
                summary.unreliable += 1;
                continue;
            }

            let (agree, diff, allowed) = if !va.is_finite() || !vb.is_finite() {
                // Undefined on both sides the same way (a pair that never
                // occurs has a null of zero spread) is agreement.
                let same = (va.is_nan() && vb.is_nan()) || va == vb;
                (same, f64::NAN, 0.0)
            } else {
                let diff = (va - vb).abs();
                let floor = tolerance.exact_atol + tolerance.exact_rtol * va.abs().max(vb.abs());
                let sigma = |row: &BTreeMap<String, f64>| {
                    row.get(&format!("{base} STD"))
                        .copied()
                        .filter(|s| s.is_finite())
                        .unwrap_or(0.0)
                };
                let spread = (sigma(row_a) + sigma(row_b)) / 2.0;
                let allowed = floor
                    + match kind {
                        Kind::Exact => 0.0,
                        Kind::Mean => tolerance.sigmas * spread * inv_n.sqrt(),
                        Kind::Std => tolerance.sigmas * spread * inv_n1.sqrt(),
                        Kind::Z => {
                            let z = (va.abs() + vb.abs()) / 2.0;
                            tolerance.sigmas * ((1.0 + z * z / 2.0) * inv_n).sqrt()
                        }
                    };
                summary.max_abs_diff = summary.max_abs_diff.max(diff);
                (diff <= allowed, diff, allowed)
            };

            if kind != Kind::Exact {
                statistical_cells += 1;
            }
            if !agree {
                summary.outside += 1;
                if kind != Kind::Exact {
                    statistical_outside += 1;
                }
                // Exact columns first in the list whatever their size: one of
                // them off says more than any number of z-scores.
                let rank = if kind == Kind::Exact {
                    f64::INFINITY
                } else if allowed > 0.0 {
                    diff / allowed
                } else {
                    f64::MAX
                };
                worst.push((
                    rank,
                    json!({
                        "sample": id,
                        "column": column,
                        "kind": kind.as_str(),
                        "a": finite_or_text(va),
                        "b": finite_or_text(vb),
                        "difference": finite_or_text(diff),
                        "tolerance": allowed,
                    }),
                ));
            }
        }
    }

    worst.sort_by(|x, y| y.0.total_cmp(&x.0));
    let worst: Vec<Value> = worst.into_iter().take(20).map(|(_, v)| v).collect();

    let expected = two_sided_tail(tolerance.sigmas) * statistical_cells as f64;
    let exact_outside = summaries.get(&Kind::Exact).map_or(0, |s| s.outside);
    let statistical_ok = (statistical_outside as f64) <= expected + 3.0 * expected.sqrt() + 1.0;
    let verdict = if exact_outside > 0 {
        "different: deterministic columns disagree, so the two runs did not see the same \
         networks or phenotypes — randomness cannot explain it"
    } else if !statistical_ok {
        "different: the permutation statistics disagree beyond Monte-Carlo noise"
    } else {
        "consistent: identical where no randomness is involved, and within Monte-Carlo noise \
         elsewhere"
    };

    let by_kind: serde_json::Map<String, Value> = summaries
        .iter()
        .map(|(kind, s)| {
            (
                kind.as_str().to_string(),
                json!({
                    "cells": s.cells,
                    "not_comparable": s.unreliable,
                    "outside_tolerance": s.outside,
                    "max_abs_difference": s.max_abs_diff,
                }),
            )
        })
        .collect();

    Ok(json!({
        "verdict": verdict,
        "tables": [path_a, path_b],
        "samples_compared": ids.len(),
        "samples_only_in_a": left.rows.keys().filter(|k| !right.rows.contains_key(*k)).collect::<Vec<_>>(),
        "samples_only_in_b": right.rows.keys().filter(|k| !left.rows.contains_key(*k)).collect::<Vec<_>>(),
        "columns_only_in_a": columns_a.difference(&columns_b).collect::<Vec<_>>(),
        "columns_only_in_b": columns_b.difference(&columns_a).collect::<Vec<_>>(),
        "n_shuffle": [na, nb],
        "sigmas": tolerance.sigmas,
        "by_kind": by_kind,
        "statistical_cells_compared": statistical_cells,
        "statistical_outliers": statistical_outside,
        "expected_false_alarms": expected,
        "worst": worst,
        "how_to_read": "exact columns (# total, %, assort, RAW) must agree to the last digit: \
                        they are counts on the network. MEAN, STD and Z come from permutations \
                        drawn by different generators in Rust and Python, so they agree only \
                        within Monte-Carlo error, which shrinks as 1/sqrt(n_shuffle). Cells \
                        counted as not_comparable belong to pairs so rare (under one edge per \
                        permutation) or so constant that their null is a spike, not a bell: \
                        no tolerance is meaningful there. The rule depends on the shape of the \
                        null, not on n_shuffle: more permutations shrink the differences but do \
                        not make these cells comparable. columns_only_in_a/b mean the phenotype \
                        sets differ.",
    }))
}

fn finite_or_text(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else {
        json!(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_classified_by_suffix_and_abundance_first() {
        assert_eq!(Kind::of("# total"), (Kind::Exact, "# total"));
        assert_eq!(Kind::of("% Z"), (Kind::Exact, "% Z"));
        assert_eq!(Kind::of("assort"), (Kind::Exact, "assort"));
        assert_eq!(Kind::of("B - T RAW"), (Kind::Exact, "B - T RAW"));
        assert_eq!(Kind::of("assort Z"), (Kind::Z, "assort"));
        assert_eq!(Kind::of("B - T MEAN"), (Kind::Mean, "B - T"));
        assert_eq!(Kind::of("B - T STD"), (Kind::Std, "B - T"));
    }

    fn row(values: &[(&str, f64)]) -> BTreeMap<String, f64> {
        values.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn rare_and_constant_nulls_are_not_compared() {
        let common = row(&[("a - b MEAN", 5.0), ("a - b STD", 1.0)]);
        let rare = row(&[("a - b MEAN", 0.3), ("a - b STD", 1.0)]);
        let constant = row(&[("a - b MEAN", 0.0), ("a - b STD", 0.0)]);
        assert_eq!(unreliable(&common, &common, "a - b"), None);
        assert!(unreliable(&common, &rare, "a - b").is_some());
        assert!(unreliable(&constant, &common, "a - b").is_some());
        // The coefficient's null is centred on zero: its mean is no measure
        // of rarity.
        let assort = row(&[("assort MEAN", 0.0001), ("assort STD", 0.003)]);
        assert_eq!(unreliable(&assort, &assort, "assort"), None);
    }

    #[test]
    fn the_normal_tail_is_right() {
        assert!((two_sided_tail(1.959_964) - 0.05).abs() < 1e-6);
        assert!((two_sided_tail(4.0) - 6.334e-5).abs() < 1e-7);
    }
}
