//! How much two partitions of the same cells agree.
//!
//! # What this is for
//!
//! A sensitivity analysis produces a run per point of a grid, each labelling
//! every cell with a niche. Counting the niches is not enough to compare them:
//! two runs can both find eight niches and disagree about half the cells, and
//! two runs can find 418 and 609 and be telling the same story at different
//! granularities. What is needed is a measure of *agreement between the
//! labellings themselves*, and the labels have no shared names — niche 3 of one
//! run has nothing to do with niche 3 of another — so it has to be built out of
//! which cells fall together rather than out of what they are called.
//!
//! # The three measures, and when each is the right one
//!
//! * [`fn@adjusted_rand_index`] counts pairs of cells: for every pair, do the
//!   two runs agree about whether they belong together? Corrected for the
//!   agreement chance alone would produce, so two unrelated partitions score
//!   about zero rather than about a half.
//!
//! * [`fn@adjusted_mutual_information`] asks how much knowing one labelling
//!   tells you about the other. Corrected the same way — and that correction is
//!   why it is preferred to the plain normalised mutual information, which
//!   rewards a partition simply for having more clusters and would call 418
//!   niches a better match for 8 than 8 is.
//!
//! The two disagree when the niches are very uneven in size: the pair count is
//! dominated by the big ones, the information measure is not. Reporting both is
//! what makes that visible.
//!
//! * [`fn@jaccard_stability`] answers a different question, and the one a
//!   biologist usually means: not "do these two runs agree" but "which of *my*
//!   niches survives when the parameter moves". For each niche of a reference
//!   run it finds the best-matching niche in another and reports the overlap.
//!   A niche that keeps a high score across a sweep is a feature of the tissue;
//!   one that dissolves is a feature of the settings.
//!
//! # A caution about reading the numbers
//!
//! On a cohort of tens of thousands of cells cut into hundreds of niches, the
//! adjusted indices are crushed towards zero: there are so many pairs of cells
//! in different niches that agreeing about them earns almost nothing. The
//! values stay useful *relative to each other* — which pair of runs agrees more
//! than which — and are not to be read as percentages.

use std::collections::BTreeMap;

use crate::error::{CoreError, Result};

/// The table of how many cells fall in niche `i` of one run and niche `j` of
/// the other, with the two margins.
struct Contingency {
    /// `(i, j) -> count`, only for pairs that occur.
    joint: BTreeMap<(u32, u32), usize>,
    rows: BTreeMap<u32, usize>,
    columns: BTreeMap<u32, usize>,
    total: usize,
}

impl Contingency {
    fn of(a: &[u32], b: &[u32]) -> Result<Self> {
        if a.len() != b.len() {
            return Err(CoreError::shape(format!(
                "two labellings of {} and {} cells cannot be compared",
                a.len(),
                b.len()
            )));
        }

        let mut joint = BTreeMap::new();
        let mut rows = BTreeMap::new();
        let mut columns = BTreeMap::new();
        for (left, right) in a.iter().zip(b) {
            *joint.entry((*left, *right)).or_insert(0) += 1;
            *rows.entry(*left).or_insert(0) += 1;
            *columns.entry(*right).or_insert(0) += 1;
        }

        Ok(Self {
            joint,
            rows,
            columns,
            total: a.len(),
        })
    }
}

/// `n * (n - 1) / 2`, the number of pairs among `n` things.
///
/// In `f64` because the sums below are of products of these, and a cohort of a
/// hundred thousand cells has five thousand million pairs.
fn pairs(n: usize) -> f64 {
    let n = n as f64;
    n * (n - 1.0) / 2.0
}

/// How much two labellings of the same cells agree, corrected for chance.
///
/// One for identical partitions, about zero for unrelated ones, and negative
/// when two partitions agree *less* than chance would predict.
///
/// Two partitions that are each a single niche agree perfectly and trivially;
/// the formula's denominator vanishes there, and one is the honest answer —
/// there is nothing the two could have disagreed about.
pub fn adjusted_rand_index(a: &[u32], b: &[u32]) -> Result<f64> {
    let table = Contingency::of(a, b)?;
    if table.total < 2 {
        return Ok(1.0);
    }

    let index: f64 = table.joint.values().map(|count| pairs(*count)).sum();
    let row_pairs: f64 = table.rows.values().map(|count| pairs(*count)).sum();
    let column_pairs: f64 = table.columns.values().map(|count| pairs(*count)).sum();
    let all = pairs(table.total);

    let expected = row_pairs * column_pairs / all;
    let maximum = (row_pairs + column_pairs) / 2.0;

    // Both partitions trivial — one niche each, or every cell its own — leaves
    // nothing to be right or wrong about, and the correction divides by zero.
    if (maximum - expected).abs() < f64::EPSILON {
        return Ok(1.0);
    }
    Ok((index - expected) / (maximum - expected))
}

/// Natural logarithms of `0!` through `n!`.
///
/// Built once per comparison and shared by every term of the expected mutual
/// information, which is otherwise the expensive part: it sums over every pair
/// of niches and every overlap they could have had.
fn log_factorials(n: usize) -> Vec<f64> {
    let mut table = Vec::with_capacity(n + 1);
    table.push(0.0);
    for k in 1..=n {
        table.push(table[k - 1] + (k as f64).ln());
    }
    table
}

/// The entropy of a labelling, in nats.
fn entropy(counts: &BTreeMap<u32, usize>, total: usize) -> f64 {
    counts
        .values()
        .filter(|count| **count > 0)
        .map(|count| {
            let p = *count as f64 / total as f64;
            -p * p.ln()
        })
        .sum()
}

/// How much knowing one labelling tells you about the other, corrected for
/// chance.
///
/// # Why this rather than the plain normalised mutual information
///
/// The unadjusted measure rewards a partition for having more clusters: split
/// every niche in two and it rises, whatever the split. Across a sweep whose
/// whole point is that the niche count moves — 418 against 8 on a real cohort —
/// that bias is not a nuisance, it is the entire signal. Subtracting what
/// chance alone would have produced removes it.
///
/// Normalised by the arithmetic mean of the two entropies, as `sklearn` does,
/// so the value is comparable with what a reader gets in Python.
pub fn adjusted_mutual_information(a: &[u32], b: &[u32]) -> Result<f64> {
    let table = Contingency::of(a, b)?;
    let n = table.total;
    if n < 2 {
        return Ok(1.0);
    }

    // Mutual information, in nats.
    let mut mutual = 0.0;
    for ((i, j), count) in &table.joint {
        let nij = *count as f64;
        let ai = table.rows[i] as f64;
        let bj = table.columns[j] as f64;
        mutual += (nij / n as f64) * ((n as f64 * nij) / (ai * bj)).ln();
    }

    let expected = expected_mutual_information(&table, &log_factorials(n));
    let (ha, hb) = (entropy(&table.rows, n), entropy(&table.columns, n));
    let normaliser = (ha + hb) / 2.0;

    // Two partitions that are each a single niche have no entropy: they agree,
    // and there was nothing to agree about.
    if (normaliser - expected).abs() < 1e-12 {
        return Ok(1.0);
    }
    Ok((mutual - expected) / (normaliser - expected))
}

/// What the mutual information would have been for two random labellings with
/// these niche sizes.
///
/// The hypergeometric expectation, summed over every overlap each pair of
/// niches could have had. Computed through log-factorials because the
/// individual terms overflow long before their ratio does.
fn expected_mutual_information(table: &Contingency, log_fact: &[f64]) -> f64 {
    let n = table.total;
    let nf = n as f64;
    let mut expected = 0.0;

    for ai in table.rows.values() {
        for bj in table.columns.values() {
            let (ai, bj) = (*ai, *bj);
            // An overlap cannot be larger than either niche, nor smaller than
            // what the two of them are forced to share in a cohort of `n`.
            let low = 1.max((ai + bj).saturating_sub(n));
            let high = ai.min(bj);

            for nij in low..=high {
                let term = (nij as f64 / nf) * ((nf * nij as f64) / (ai as f64 * bj as f64)).ln();
                // log P(nij) under the hypergeometric model.
                // `n + nij - ai - bj`, and in that order: the low bound of the
                // loop guarantees the result is not negative, but `n - ai - bj`
                // on its own underflows a `usize` on the way there.
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
    expected
}

/// How well each niche of `reference` survives in `other`.
///
/// For every niche of the reference, the best Jaccard overlap it has with any
/// niche of the other run: the number of cells the two share over the number in
/// either. One means the niche is found again exactly; zero means no niche of
/// the other run resembles it at all.
///
/// # Why the best match rather than the matching
///
/// A one-to-one assignment would be tidier and would answer a question nobody
/// asked. The question is whether *this* niche is still there, and it is still
/// there if something in the other run holds the same cells — whether or not
/// that something is also the best match for a different niche. Two reference
/// niches that both map to one merged niche is exactly the case worth seeing,
/// and a one-to-one matching would hide half of it.
///
/// Returned in niche order, alongside the niche's own label, so a caller can
/// name the rows of the figure it draws.
pub fn jaccard_stability(reference: &[u32], other: &[u32]) -> Result<Vec<(u32, f64)>> {
    let table = Contingency::of(reference, other)?;

    Ok(table
        .rows
        .iter()
        .map(|(niche, size)| {
            let best = table
                .columns
                .keys()
                .map(|theirs| {
                    let shared = table.joint.get(&(*niche, *theirs)).copied().unwrap_or(0);
                    if shared == 0 {
                        return 0.0;
                    }
                    let union = size + table.columns[theirs] - shared;
                    shared as f64 / union as f64
                })
                .fold(0.0, f64::max);
            (*niche, best)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    // -----------------------------------------------------------------------
    // The adjusted Rand index
    // -----------------------------------------------------------------------

    #[test]
    fn a_partition_agrees_perfectly_with_itself() {
        let labels = [0, 0, 1, 1, 2, 2, 2];
        assert!(close(adjusted_rand_index(&labels, &labels).unwrap(), 1.0));
    }

    /// The labels have no shared names: niche 3 of one run has nothing to do
    /// with niche 3 of another, so only which cells fall together counts.
    #[test]
    fn renaming_the_niches_changes_nothing() {
        let a = [0, 0, 1, 1, 2, 2];
        let b = [7, 7, 0, 0, 4, 4];
        assert!(close(adjusted_rand_index(&a, &b).unwrap(), 1.0));
    }

    /// The value `sklearn.metrics.adjusted_rand_score` returns for the example
    /// in its own documentation.
    #[test]
    fn the_index_matches_the_reference_implementation() {
        let a = [0, 0, 1, 1];
        let b = [0, 1, 0, 1];
        assert!(
            close(adjusted_rand_index(&a, &b).unwrap(), -0.5),
            "{}",
            adjusted_rand_index(&a, &b).unwrap()
        );

        let a = [0, 0, 1, 2];
        let b = [0, 0, 1, 1];
        let got = adjusted_rand_index(&a, &b).unwrap();
        assert!(close(got, 0.5714285714285715), "{got}");
    }

    /// Splitting a niche in two is a disagreement, and a measured one.
    #[test]
    fn splitting_a_niche_lowers_the_index() {
        let coarse = [0, 0, 0, 0, 1, 1, 1, 1];
        let fine = [0, 0, 1, 1, 2, 2, 3, 3];
        let got = adjusted_rand_index(&coarse, &fine).unwrap();
        assert!(got > 0.0 && got < 1.0, "{got}");
    }

    /// Two partitions that are each one niche agree, and there was nothing to
    /// disagree about — the correction's denominator vanishes there.
    #[test]
    fn two_trivial_partitions_agree_rather_than_divide_by_zero() {
        let a = [0, 0, 0, 0];
        let b = [5, 5, 5, 5];
        assert!(close(adjusted_rand_index(&a, &b).unwrap(), 1.0));
    }

    #[test]
    fn a_single_cell_is_a_degenerate_comparison_not_an_error() {
        assert!(close(adjusted_rand_index(&[0], &[9]).unwrap(), 1.0));
        assert!(close(adjusted_rand_index(&[], &[]).unwrap(), 1.0));
    }

    /// Two labellings of different cohorts are not comparable, and saying so is
    /// better than comparing the first `n` of each.
    #[test]
    fn labellings_of_different_lengths_are_refused() {
        let error = adjusted_rand_index(&[0, 1], &[0, 1, 2]).unwrap_err();
        assert!(error.to_string().contains("cannot be compared"), "{error}");
    }

    // -----------------------------------------------------------------------
    // The adjusted mutual information
    // -----------------------------------------------------------------------

    #[test]
    fn the_information_of_a_partition_with_itself_is_one() {
        let labels = [0, 0, 1, 1, 2, 2];
        let got = adjusted_mutual_information(&labels, &labels).unwrap();
        assert!(close(got, 1.0), "{got}");
    }

    #[test]
    fn renaming_the_niches_changes_the_information_not_at_all() {
        let a = [0, 0, 1, 1, 2, 2];
        let b = [3, 3, 9, 9, 1, 1];
        assert!(close(adjusted_mutual_information(&a, &b).unwrap(), 1.0));
    }

    /// The correction is the whole point: two unrelated labellings score about
    /// zero rather than about a half.
    #[test]
    fn two_unrelated_labellings_score_about_zero() {
        // Deterministic, and uncorrelated by construction.
        let a: Vec<u32> = (0..120).map(|i| i % 4).collect();
        let b: Vec<u32> = (0..120).map(|i| (i / 4) % 3).collect();

        let ami = adjusted_mutual_information(&a, &b).unwrap();
        assert!(ami.abs() < 0.05, "{ami}");
    }

    /// And the bias it removes: splitting every niche in two raises the
    /// *unadjusted* information towards one while the adjusted one does not
    /// pretend the finer partition is a better match.
    #[test]
    fn splitting_every_niche_does_not_earn_a_perfect_score() {
        let coarse: Vec<u32> = (0..64).map(|i| i / 32).collect();
        let fine: Vec<u32> = (0..64).map(|i| i / 8).collect();

        let ami = adjusted_mutual_information(&coarse, &fine).unwrap();
        assert!(ami > 0.0, "{ami}");
        assert!(
            ami < 0.999,
            "a finer partition scored as a perfect match: {ami}"
        );
    }

    #[test]
    fn the_information_of_two_trivial_partitions_is_one() {
        assert!(close(
            adjusted_mutual_information(&[0, 0, 0], &[1, 1, 1]).unwrap(),
            1.0
        ));
    }

    // -----------------------------------------------------------------------
    // Jaccard stability
    // -----------------------------------------------------------------------

    /// A niche found again exactly scores one.
    #[test]
    fn a_niche_that_survives_intact_scores_one() {
        let reference = [0, 0, 0, 1, 1, 1];
        let other = [4, 4, 4, 9, 9, 9];

        let stability = jaccard_stability(&reference, &other).unwrap();
        assert_eq!(stability, vec![(0, 1.0), (1, 1.0)]);
    }

    /// A niche split in two keeps the larger half, and says so.
    #[test]
    fn a_niche_that_is_split_keeps_the_share_of_its_larger_half() {
        let reference = [0, 0, 0, 0];
        let other = [1, 1, 1, 2];

        // Three of the four cells land together; the union is four.
        let stability = jaccard_stability(&reference, &other).unwrap();
        assert_eq!(stability.len(), 1);
        assert!(close(stability[0].1, 0.75), "{:?}", stability);
    }

    /// A niche absorbed into a much larger one scores low, which is the point:
    /// it did not survive as itself.
    #[test]
    fn a_niche_swallowed_by_a_larger_one_scores_low() {
        let reference: Vec<u32> = (0..10).map(|i| u32::from(i >= 2)).collect();
        let other = vec![0u32; 10];

        let stability = jaccard_stability(&reference, &other).unwrap();
        // Niche 0 holds two cells, and the union with the other run's single
        // niche of ten is ten.
        assert!(close(stability[0].1, 0.2), "{stability:?}");
        assert!(close(stability[1].1, 0.8), "{stability:?}");
    }

    /// The rows are named, so a figure can label them with the niche they are.
    #[test]
    fn the_scores_are_returned_beside_the_niche_they_belong_to() {
        let reference = [3, 3, 7, 7];
        let other = [0, 0, 1, 1];

        let stability = jaccard_stability(&reference, &other).unwrap();
        assert_eq!(
            stability
                .iter()
                .map(|(niche, _)| *niche)
                .collect::<Vec<_>>(),
            vec![3, 7]
        );
    }

    /// Two reference niches that merge into one both report the overlap they
    /// have with it — a one-to-one matching would have hidden half of it.
    #[test]
    fn two_niches_that_merge_both_report_their_overlap() {
        let reference = [0, 0, 1, 1];
        let other = [5, 5, 5, 5];

        let stability = jaccard_stability(&reference, &other).unwrap();
        assert!(close(stability[0].1, 0.5), "{stability:?}");
        assert!(close(stability[1].1, 0.5), "{stability:?}");
    }

    #[test]
    fn stability_refuses_labellings_of_different_lengths() {
        assert!(jaccard_stability(&[0], &[0, 1]).is_err());
    }

    /// Both indices, against values computed independently from the published
    /// formulae. They are what `sklearn.metrics.adjusted_rand_score` and
    /// `adjusted_mutual_info_score` return for the same inputs, so a reader
    /// moving between this and Python gets the same numbers.
    #[test]
    fn both_indices_match_the_published_formulae() {
        let cases: [(&[u32], &[u32], f64, f64); 3] = [
            (&[0, 0, 1, 1], &[0, 1, 0, 1], -0.5, -0.5),
            (
                &[0, 0, 1, 2],
                &[0, 0, 1, 1],
                0.571_428_571_429,
                0.571_428_571_429,
            ),
            (
                &[0, 0, 0, 0, 1, 1, 1, 1],
                &[0, 0, 1, 1, 2, 2, 3, 3],
                0.363_636_363_636,
                0.533_333_333_333,
            ),
        ];

        for (a, b, want_ari, want_ami) in cases {
            let got_ari = adjusted_rand_index(a, b).unwrap();
            let got_ami = adjusted_mutual_information(a, b).unwrap();
            assert!(
                (got_ari - want_ari).abs() < 1e-9,
                "ARI {got_ari} vs {want_ari}"
            );
            assert!(
                (got_ami - want_ami).abs() < 1e-9,
                "AMI {got_ami} vs {want_ami}"
            );
        }
    }

    /// The two disagree when the niches are uneven, which is why both are
    /// reported: the pair count is dominated by the large niches and the
    /// information measure is not.
    #[test]
    fn the_two_indices_disagree_where_they_should() {
        let coarse: Vec<u32> = (0..64).map(|i| i / 32).collect();
        let fine: Vec<u32> = (0..64).map(|i| i / 8).collect();

        let ari = adjusted_rand_index(&coarse, &fine).unwrap();
        let ami = adjusted_mutual_information(&coarse, &fine).unwrap();
        assert!((ari - 0.228_571_428_571).abs() < 1e-9, "{ari}");
        assert!((ami - 0.477_571_925_112).abs() < 1e-9, "{ami}");
        assert!(
            ami > ari,
            "the information measure is the forgiving one here"
        );
    }
}
