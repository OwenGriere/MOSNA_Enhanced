//! Comparing the runs of a sweep against one another.
//!
//! # What this reads, and why it needs nothing else
//!
//! Every run of a sweep writes its niche label for every cell back into the
//! nodes files, under a column named after the run — `niches_1-1-3`. Those
//! columns are all on the same cells, in the same order, so they can be
//! compared directly. Nothing has to be recomputed and no run has to be kept in
//! memory: the comparison is a second pass over files that are already on disk,
//! and it works just as well on a sweep from last week.
//!
//! # The three questions it answers
//!
//! * **Where does the answer change?** The agreement between *consecutive* runs
//!   of the sweep, in the order the grid was walked. A flat line says the
//!   parameter is doing nothing over that stretch; a dip is where the structure
//!   actually moves. The niche count alone misses this — a run can go from
//!   eight niches to eight niches while reassigning half the cells.
//!
//! * **Which runs agree with which?** Every run against every other, as a
//!   matrix. Blocks in it are families of settings that tell the same story,
//!   and that is the reading a sweep over a categorical parameter needs, where
//!   there is no order to walk along.
//!
//! * **Which niches survive?** For each niche of a reference run, how well it
//!   is found again in every other. This is the one a biologist usually wants:
//!   not whether two runs agree, but which of the niches in front of them is a
//!   feature of the tissue rather than of the settings.
//!
//! The matrix is the adjusted Rand index alone. Its corrected mutual
//! information counterpart costs a sum over every pair of niches and every
//! overlap they could have had, which is affordable for the handful of
//! consecutive pairs and not for the full square of a sweep with fifty runs.

use std::path::Path;

use mosna_core::niches::{adjusted_mutual_information, adjusted_rand_index, jaccard_stability};

use crate::error::Result;
use crate::figures::FigureSink;
use crate::progress::Progress;
use mosna_io::read::get_opener::{read_table_columns, Extension};
use mosna_io::SampleId;

/// The niche label of every cell, for one run.
///
/// The samples are concatenated in the order the analysis stacked them, which
/// is `data_index` order — the same order every run used, so two of these are
/// aligned cell for cell.
#[derive(Debug, Clone)]
pub struct RunLabels {
    pub run: String,
    pub labels: Vec<u32>,
}

/// Agreement between two runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Agreement {
    /// The two runs, as `1-1-1 → 1-1-2`.
    pub pair: String,
    pub ari: f64,
    pub ami: f64,
}

/// How well each niche of the reference run is found again elsewhere.
#[derive(Debug, Clone, PartialEq)]
pub struct Stability {
    /// The reference run its niches belong to.
    pub reference: String,
    /// One row per niche of the reference, named by its label.
    pub niches: Vec<u32>,
    /// One column per other run.
    pub runs: Vec<String>,
    /// Row-major, `niches.len()` by `runs.len()`.
    pub values: Vec<f64>,
}

/// Everything the comparison found.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub runs: Vec<String>,
    /// Consecutive pairs, in the order the grid was walked.
    pub adjacent: Vec<Agreement>,
    /// Every run against every other, row-major and square.
    pub matrix: Vec<f64>,
    pub stability: Stability,
}

impl Comparison {
    /// Whether there is anything to draw.
    ///
    /// One run compares with nothing, and a sweep of one is not a sensitivity
    /// analysis.
    pub fn is_empty(&self) -> bool {
        self.runs.len() < 2
    }
}

/// Read back the niche labels of every run, in the order the cells were
/// stacked.
///
/// A run whose column is missing from any sample is skipped rather than
/// reported: it is a run that failed, or one whose network was rebuilt after
/// it, and neither is a reason to refuse the comparison of the rest.
pub fn read_labels(
    net_dir: &Path,
    patient_column: &str,
    sample_column: Option<&str>,
    runs: &[String],
) -> Vec<RunLabels> {
    let Ok(index) = mosna_io::make_data_index(net_dir, patient_column, sample_column, "parquet")
    else {
        return Vec::new();
    };

    runs.iter()
        .filter_map(|run| {
            let column = format!("niches_{run}");
            let labels = read_column(net_dir, &index, patient_column, sample_column, &column)?;
            Some(RunLabels {
                run: run.clone(),
                labels,
            })
        })
        .collect()
}

/// One column, concatenated across the cohort.
fn read_column(
    net_dir: &Path,
    index: &[SampleId],
    patient_column: &str,
    sample_column: Option<&str>,
    column: &str,
) -> Option<Vec<u32>> {
    let mut all = Vec::new();
    for id in index {
        let path = net_dir.join(id.nodes_file_name(patient_column, sample_column, "parquet"));
        let table = read_table_columns(&path, Extension::Parquet, &[column]).ok()?;
        // Written as `u32` and read back as `f64`, which is how every other
        // reader in the interface takes it.
        let values = table.f64_column(column).ok()?;
        all.extend(values.into_iter().map(|value| value as u32));
    }
    (!all.is_empty()).then_some(all)
}

/// Compare every run with its neighbour, with every other, and with a
/// reference.
///
/// `reference` is the run the stability is measured against; the first is used
/// when it is not among the runs, because the first point of a grid is the one
/// a reader has usually already looked at.
pub fn compare(labels: &[RunLabels], reference: Option<&str>) -> Comparison {
    let runs: Vec<String> = labels.iter().map(|entry| entry.run.clone()).collect();

    // Consecutive pairs, in the order the grid was walked.
    let adjacent = labels
        .windows(2)
        .map(|pair| {
            let (left, right) = (&pair[0], &pair[1]);
            Agreement {
                pair: format!("{} → {}", left.run, right.run),
                ari: adjusted_rand_index(&left.labels, &right.labels).unwrap_or(f64::NAN),
                ami: adjusted_mutual_information(&left.labels, &right.labels).unwrap_or(f64::NAN),
            }
        })
        .collect();

    // The full square. Symmetric, so only half of it is computed.
    let n = labels.len();
    let mut matrix = vec![1.0; n * n];
    for i in 0..n {
        for j in (i + 1)..n {
            let value =
                adjusted_rand_index(&labels[i].labels, &labels[j].labels).unwrap_or(f64::NAN);
            matrix[i * n + j] = value;
            matrix[j * n + i] = value;
        }
    }

    let stability = stability_of(labels, reference);
    Comparison {
        runs,
        adjacent,
        matrix,
        stability,
    }
}

/// How well each niche of the reference run survives in every other.
fn stability_of(labels: &[RunLabels], reference: Option<&str>) -> Stability {
    let Some(anchor) = reference
        .and_then(|name| labels.iter().find(|entry| entry.run == name))
        .or_else(|| labels.first())
    else {
        return Stability {
            reference: String::new(),
            niches: Vec::new(),
            runs: Vec::new(),
            values: Vec::new(),
        };
    };

    // The reference against itself is one by construction, and a column of ones
    // is a column that says nothing; it is left out.
    let others: Vec<&RunLabels> = labels
        .iter()
        .filter(|entry| entry.run != anchor.run)
        .collect();

    let columns: Vec<Vec<(u32, f64)>> = others
        .iter()
        .map(|other| jaccard_stability(&anchor.labels, &other.labels).unwrap_or_default())
        .collect();

    let niches: Vec<u32> = columns
        .first()
        .map(|column| column.iter().map(|(niche, _)| *niche).collect())
        .unwrap_or_default();

    let mut values = vec![f64::NAN; niches.len() * others.len()];
    for (column_index, column) in columns.iter().enumerate() {
        for (row_index, (_, score)) in column.iter().enumerate() {
            if row_index < niches.len() {
                values[row_index * others.len() + column_index] = *score;
            }
        }
    }

    Stability {
        reference: anchor.run.clone(),
        niches,
        runs: others.iter().map(|entry| entry.run.clone()).collect(),
        values,
    }
}

/// Compare the runs of a sweep, and draw the three figures that read it.
///
/// # Why this is a step of its own
///
/// Every measure is between *pairs* of runs, so none of them exists until the
/// sweep does: computing them as it went would mean redoing the whole square
/// each time a run landed. And making it a command rather than a phase of the
/// sweep means it can be pointed at a sweep from last week — the label columns
/// are on disk, and nothing else is needed.
///
/// A comparison that cannot be made is not a failure. The runs themselves
/// succeeded and their results are where they were left; a sweep of one run
/// has nothing to compare, and says so.
pub fn compare_sweep(
    working_dir: &Path,
    results_in: &str,
    patient_column: &str,
    sample_column: Option<&str>,
    progress: &dyn Progress,
    figures: &dyn FigureSink,
) -> Result<()> {
    let niche_dir = working_dir.join("Niche_Analysis");
    let save_dir = niche_dir.join(results_in);
    let runs = runs_in(&save_dir);

    if runs.len() < 2 {
        progress.info("[INFO] Fewer than two runs to compare: a sensitivity analysis needs a grid");
        return Ok(());
    }

    progress.info(&format!("[INFO] Comparing {} runs", runs.len()));
    progress.step(0, 2, "[PROCESS] Comparing the runs");

    let net_dir = working_dir.join("temp/net_dir_mosna");
    let labels = read_labels(&net_dir, patient_column, sample_column, &runs);
    if labels.len() < 2 {
        progress.info(
            "[INFO] The runs' label columns are not in the network files; nothing to compare",
        );
        return Ok(());
    }

    let comparison = compare(&labels, None);
    progress.step(1, 2, "[PROCESS] Drawing the comparison");

    let pairs: Vec<String> = comparison
        .adjacent
        .iter()
        .map(|step| step.pair.clone())
        .collect();
    let ari: Vec<f64> = comparison.adjacent.iter().map(|step| step.ari).collect();
    let ami: Vec<f64> = comparison.adjacent.iter().map(|step| step.ami).collect();

    figures.sensitivity(
        &pairs,
        &ari,
        &ami,
        &comparison.runs,
        &comparison.matrix,
        &comparison.stability.reference,
        &comparison.stability.niches,
        &comparison.stability.runs,
        &comparison.stability.values,
        &save_dir,
    )?;
    progress.step(2, 2, "[PROCESS] Comparing the runs");
    Ok(())
}

/// The run directories of a sweep, in the order their numbers put them.
///
/// Numerically rather than alphabetically: `1-1-10` comes after `1-1-2`, and
/// the order is the order the grid was walked, which is what the agreement
/// between consecutive runs means.
fn runs_in(save_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(save_dir) else {
        return Vec::new();
    };
    let mut runs: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();

    runs.sort_by_key(|run| {
        let numbers: Vec<u64> = run
            .split(['-', '_'])
            .filter_map(|part| part.parse().ok())
            .collect();
        (numbers.is_empty(), numbers, run.clone())
    });
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, labels: &[u32]) -> RunLabels {
        RunLabels {
            run: name.to_string(),
            labels: labels.to_vec(),
        }
    }

    /// Three runs: the first two identical, the third quite different.
    fn three() -> Vec<RunLabels> {
        vec![
            run("1-1-1", &[0, 0, 0, 1, 1, 1]),
            run("1-1-2", &[5, 5, 5, 9, 9, 9]),
            run("1-1-3", &[0, 1, 0, 1, 0, 1]),
        ]
    }

    /// The question the niche count cannot answer: where along the sweep does
    /// the answer actually change?
    #[test]
    fn consecutive_runs_are_compared_in_the_order_the_grid_was_walked() {
        let comparison = compare(&three(), None);

        assert_eq!(comparison.adjacent.len(), 2, "one pair per step");
        assert_eq!(comparison.adjacent[0].pair, "1-1-1 → 1-1-2");
        assert!(
            (comparison.adjacent[0].ari - 1.0).abs() < 1e-9,
            "renaming the niches is not a disagreement"
        );
        assert!(
            comparison.adjacent[1].ari < 0.5,
            "a real change was not seen: {}",
            comparison.adjacent[1].ari
        );
    }

    /// Both measures, because they disagree where the niches are uneven and
    /// that disagreement is worth seeing.
    #[test]
    fn each_consecutive_pair_carries_both_measures() {
        let comparison = compare(&three(), None);
        for step in &comparison.adjacent {
            assert!(step.ari.is_finite(), "{step:?}");
            assert!(step.ami.is_finite(), "{step:?}");
        }
    }

    /// The matrix is square, symmetric, and one down its diagonal: a run agrees
    /// with itself.
    #[test]
    fn the_matrix_is_symmetric_with_agreement_down_its_diagonal() {
        let comparison = compare(&three(), None);
        let n = comparison.runs.len();
        assert_eq!(comparison.matrix.len(), n * n);

        for i in 0..n {
            assert!((comparison.matrix[i * n + i] - 1.0).abs() < 1e-12);
            for j in 0..n {
                assert!(
                    (comparison.matrix[i * n + j] - comparison.matrix[j * n + i]).abs() < 1e-12,
                    "the matrix is not symmetric at ({i}, {j})"
                );
            }
        }
    }

    /// The stability is measured against one run, and that run is not a column
    /// of it: a column of ones says nothing.
    #[test]
    fn the_stability_leaves_the_reference_out_of_its_own_columns() {
        let comparison = compare(&three(), None);
        let stability = &comparison.stability;

        assert_eq!(stability.reference, "1-1-1");
        assert_eq!(stability.runs, vec!["1-1-2", "1-1-3"]);
        assert_eq!(stability.niches, vec![0, 1], "one row per reference niche");
        assert_eq!(stability.values.len(), 2 * 2);
    }

    /// A niche found again exactly scores one; one shredded across the other
    /// run's niches scores low. That contrast is the whole figure.
    #[test]
    fn a_niche_that_survives_scores_high_and_one_that_dissolves_scores_low() {
        let comparison = compare(&three(), None);
        let stability = &comparison.stability;

        // Row-major, two runs per row. Against `1-1-2`, which is the same
        // partition renamed, the niche is found again exactly.
        assert!((stability.values[0] - 1.0).abs() < 1e-9, "{stability:?}");

        // Against `1-1-3`, which cuts across it: niche 0 holds cells 0, 1 and
        // 2, and the best it finds there holds 0, 2 and 4 — two shared out of
        // four between them.
        assert!((stability.values[1] - 0.5).abs() < 1e-9, "{stability:?}");
        assert!(
            stability.values[1] < stability.values[0],
            "the contrast the figure is read by is not there"
        );
    }

    /// The reference can be chosen: a reader who has settled on a run wants the
    /// others measured against *that* one.
    #[test]
    fn the_reference_run_can_be_chosen() {
        let comparison = compare(&three(), Some("1-1-3"));
        assert_eq!(comparison.stability.reference, "1-1-3");
        assert_eq!(comparison.stability.runs, vec!["1-1-1", "1-1-2"]);
    }

    /// A reference that is not among the runs falls back to the first rather
    /// than producing nothing.
    #[test]
    fn an_unknown_reference_falls_back_to_the_first_run() {
        let comparison = compare(&three(), Some("9-9-9"));
        assert_eq!(comparison.stability.reference, "1-1-1");
    }

    /// One run has nothing to be compared with, and a sweep of one is not a
    /// sensitivity analysis.
    #[test]
    fn a_single_run_is_nothing_to_compare() {
        let comparison = compare(&[run("1-1-1", &[0, 1])], None);
        assert!(comparison.is_empty());
        assert!(comparison.adjacent.is_empty());
        assert!(comparison.stability.runs.is_empty());
    }

    #[test]
    fn no_runs_at_all_is_not_a_panic() {
        let comparison = compare(&[], None);
        assert!(comparison.is_empty());
        assert!(comparison.matrix.is_empty());
        assert_eq!(comparison.stability.reference, "");
    }

    // -----------------------------------------------------------------------
    // Reading the columns back
    // -----------------------------------------------------------------------

    /// The labels come from the nodes files, concatenated in the order the
    /// analysis stacked the samples — the same order for every run, which is
    /// what makes two of them comparable cell for cell.
    #[test]
    fn the_labels_are_read_back_in_the_order_the_cells_were_stacked() {
        let dir = tempfile::tempdir().unwrap();
        for (sample, labels) in [(1u32, [0u32, 0, 1]), (2, [1, 1, 0])] {
            let table = mosna_io::Table::from_columns(vec![
                (
                    "Cluster".into(),
                    mosna_io::Table::string_array(["A", "B", "A"]),
                ),
                (
                    "niches_1-1-1".into(),
                    mosna_io::Table::u32_array(labels.iter().copied()),
                ),
            ])
            .unwrap();
            mosna_io::write::write_parquet::write_parquet(
                &table,
                dir.path().join(format!("nodes_patient-{sample}.parquet")),
            )
            .unwrap();
        }

        let read = read_labels(dir.path(), "patient", None, &["1-1-1".to_string()]);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].run, "1-1-1");
        assert_eq!(read[0].labels, vec![0, 0, 1, 1, 1, 0]);
    }

    /// A run whose column is not there — it failed, or the network was rebuilt
    /// after it — is skipped rather than refusing the comparison of the rest.
    #[test]
    fn a_run_whose_column_is_missing_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let table = mosna_io::Table::from_columns(vec![(
            "niches_1-1-1".into(),
            mosna_io::Table::u32_array([0u32, 1]),
        )])
        .unwrap();
        mosna_io::write::write_parquet::write_parquet(
            &table,
            dir.path().join("nodes_patient-1.parquet"),
        )
        .unwrap();

        let read = read_labels(
            dir.path(),
            "patient",
            None,
            &["1-1-1".to_string(), "1-1-9".to_string()],
        );
        assert_eq!(read.len(), 1, "the absent run was not skipped");
        assert_eq!(read[0].run, "1-1-1");
    }

    /// A directory with no networks in it yields nothing rather than failing:
    /// the comparison is an extra, and it not being possible is not an error
    /// that should reach the user as a dialog.
    #[test]
    fn an_empty_directory_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_labels(dir.path(), "patient", None, &["1-1-1".to_string()]).is_empty());
    }
}
