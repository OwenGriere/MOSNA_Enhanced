//! Agreement between two partitions of the same cells, corrected for chance.
//!
//! The same two indices as `mosna_core::niches::agreement`, carried here so the
//! server depends on nothing MOSNA_GUI's workspace lacks: the crate is copied
//! unchanged into both repositories. Both follow `sklearn`
//! (`adjusted_rand_score`, `adjusted_mutual_info_score` with the arithmetic
//! mean), so a value computed here reads like one computed in Python.

use std::collections::BTreeMap;

/// Cells in niche `i` of one labelling and `j` of the other, with the margins.
struct Contingency {
    joint: BTreeMap<(u32, u32), usize>,
    rows: BTreeMap<u32, usize>,
    columns: BTreeMap<u32, usize>,
    total: usize,
}

impl Contingency {
    fn of(a: &[u32], b: &[u32]) -> anyhow::Result<Self> {
        if a.len() != b.len() {
            anyhow::bail!(
                "two labellings of {} and {} cells cannot be compared",
                a.len(),
                b.len()
            );
        }
        let mut table = Contingency {
            joint: BTreeMap::new(),
            rows: BTreeMap::new(),
            columns: BTreeMap::new(),
            total: a.len(),
        };
        for (left, right) in a.iter().zip(b) {
            *table.joint.entry((*left, *right)).or_insert(0) += 1;
            *table.rows.entry(*left).or_insert(0) += 1;
            *table.columns.entry(*right).or_insert(0) += 1;
        }
        Ok(table)
    }
}

/// `n (n - 1) / 2`, in `f64`: a cohort of 10⁵ cells has 5·10⁹ pairs.
fn pairs(n: usize) -> f64 {
    let n = n as f64;
    n * (n - 1.0) / 2.0
}

/// Adjusted Rand index: 1 for the same partition up to renaming, about 0 for
/// unrelated ones.
pub fn adjusted_rand_index(a: &[u32], b: &[u32]) -> anyhow::Result<f64> {
    let table = Contingency::of(a, b)?;
    if table.total < 2 {
        return Ok(1.0);
    }
    let index: f64 = table.joint.values().map(|c| pairs(*c)).sum();
    let row_pairs: f64 = table.rows.values().map(|c| pairs(*c)).sum();
    let column_pairs: f64 = table.columns.values().map(|c| pairs(*c)).sum();
    let expected = row_pairs * column_pairs / pairs(table.total);
    let maximum = (row_pairs + column_pairs) / 2.0;
    // Both trivial (one niche each, or one cell per niche): nothing to disagree
    // about, and the correction would divide by zero.
    if (maximum - expected).abs() < f64::EPSILON {
        return Ok(1.0);
    }
    Ok((index - expected) / (maximum - expected))
}

fn log_factorials(n: usize) -> Vec<f64> {
    let mut table = Vec::with_capacity(n + 1);
    table.push(0.0);
    for k in 1..=n {
        table.push(table[k - 1] + (k as f64).ln());
    }
    table
}

fn entropy(counts: &BTreeMap<u32, usize>, total: usize) -> f64 {
    counts
        .values()
        .map(|c| {
            let p = *c as f64 / total as f64;
            -p * p.ln()
        })
        .sum()
}

/// Adjusted mutual information, normalised by the arithmetic mean of the two
/// entropies.
///
/// Unlike the plain normalised mutual information it does not reward a
/// partition for simply having more niches.
pub fn adjusted_mutual_information(a: &[u32], b: &[u32]) -> anyhow::Result<f64> {
    let table = Contingency::of(a, b)?;
    let n = table.total;
    if n < 2 {
        return Ok(1.0);
    }
    let nf = n as f64;

    let mutual: f64 = table
        .joint
        .iter()
        .map(|((i, j), count)| {
            let nij = *count as f64;
            let (ai, bj) = (table.rows[i] as f64, table.columns[j] as f64);
            (nij / nf) * ((nf * nij) / (ai * bj)).ln()
        })
        .sum();

    let log_fact = log_factorials(n);
    let mut expected = 0.0;
    for ai in table.rows.values().copied() {
        for bj in table.columns.values().copied() {
            let low = 1.max((ai + bj).saturating_sub(n));
            for nij in low..=ai.min(bj) {
                let term = (nij as f64 / nf) * ((nf * nij as f64) / (ai as f64 * bj as f64)).ln();
                // `n + nij - ai - bj` in that order: `n - ai - bj` alone can
                // underflow on the way to a non-negative result.
                let rest = n + nij - ai - bj;
                let log_p = log_fact[ai] + log_fact[bj] + log_fact[n - ai] + log_fact[n - bj]
                    - log_fact[n]
                    - log_fact[nij]
                    - log_fact[ai - nij]
                    - log_fact[bj - nij]
                    - log_fact[rest];
                expected += term * log_p.exp();
            }
        }
    }

    let normaliser = (entropy(&table.rows, n) + entropy(&table.columns, n)) / 2.0;
    if (normaliser - expected).abs() < 1e-12 {
        return Ok(1.0);
    }
    Ok((mutual - expected) / (normaliser - expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renaming_does_not_change_agreement() {
        let a = [0, 0, 1, 1, 2, 2];
        let b = [5, 5, 3, 3, 9, 9];
        assert!((adjusted_rand_index(&a, &b).unwrap() - 1.0).abs() < 1e-12);
        assert!((adjusted_mutual_information(&a, &b).unwrap() - 1.0).abs() < 1e-12);
    }

    /// Reference values computed with scikit-learn in the mosna-GUI environment.
    #[test]
    fn values_match_scikit_learn() {
        let a = [0, 0, 0, 1, 1, 1];
        let b = [0, 0, 1, 1, 2, 2];
        assert!((adjusted_rand_index(&a, &b).unwrap() - 0.242_424_242_424_242_4).abs() < 1e-12);
        assert!(
            (adjusted_mutual_information(&a, &b).unwrap() - 0.298_792_458_170_890_1).abs() < 1e-9
        );
    }

    #[test]
    fn different_lengths_are_refused() {
        assert!(adjusted_rand_index(&[0, 1], &[0]).is_err());
    }
}
