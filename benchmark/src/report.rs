//! Turning measurements into something a reader can act on.
//!
//! Markdown, because the results are meant to be committed next to the code and
//! read in a diff: a table whose columns line up shows at a glance which row
//! moved between two runs.

use std::time::Duration;

use crate::timing::{human_bytes, human_duration, Samples};

/// One measured stage at one cohort size.
#[derive(Debug, Clone)]
pub struct Timing {
    pub stage: String,
    pub cells: usize,
    pub samples: Samples,
    pub peak_rss: Option<u64>,
}

impl Timing {
    /// Time per cell, which is what says whether the port scales linearly or
    /// something quadratic is hiding.
    pub fn per_cell(&self) -> Duration {
        if self.cells == 0 {
            return Duration::ZERO;
        }
        self.samples.median() / self.cells as u32
    }
}

/// A markdown table of timings.
pub fn timings(rows: &[Timing]) -> String {
    let mut out = String::from(
        "| Stage | Cells | Median | Spread | Fastest | Per cell | Peak RSS |\n\
         |---|---:|---:|---:|---:|---:|---:|\n",
    );
    for row in rows {
        out.push_str(&format!(
            "| {} | {} | {} | ± {} | {} | {} | {} |\n",
            row.stage,
            thousands(row.cells),
            human_duration(row.samples.median()),
            human_duration(row.samples.mad()),
            human_duration(row.samples.best()),
            human_duration(row.per_cell()),
            row.peak_rss.map(human_bytes).unwrap_or_else(|| "—".into()),
        ));
    }
    out
}

/// The verdict of level 1.
pub fn golden(differences: &[String]) -> String {
    if differences.is_empty() {
        return "Level 1 — golden: **no drift**. Every deterministic stage \
                reproduces the recorded reference.\n"
            .to_string();
    }

    let mut out = format!(
        "Level 1 — golden: **{} stage(s) moved**.\n\n",
        differences.len()
    );
    for difference in differences {
        out.push_str(&format!("- {difference}\n"));
    }
    out.push_str(
        "\nIf the change was intended, re-record with `--update`; the diff of \
         `golden/` then shows exactly what moved.\n",
    );
    out
}

/// The verdict of level 2.
pub fn reproducibility(result: &crate::levels::Reproducibility) -> String {
    if result.is_reproducible() {
        return "Level 2 — reproducibility: **holds**. On one thread and on many, \
                the partitions are identical label for label and the numbers agree \
                to the reduction tolerance.\n"
            .to_string();
    }

    let mut out = format!(
        "Level 2 — reproducibility: **{} stage(s) depend on the thread count**.\n\n",
        result.differences.len()
    );
    for difference in &result.differences {
        out.push_str(&format!("- {difference}\n"));
    }
    out
}

/// The verdict of level 3.
pub fn recovery(result: &crate::levels::Recovery) -> String {
    format!(
        "Level 3 — recovery of the planted niches:\n\n\
         | Measure | Value |\n\
         |---|---:|\n\
         | Adjusted Rand index | {:.3} |\n\
         | Normalised mutual information | {:.3} |\n\
         | Neighbourhood overlap after projection | {:.3} |\n\
         | Niches found | {} |\n\
         | Niches planted | {} |\n\n\
         The Rand index is corrected for chance: 1 is a perfect match, 0 is no \
         better than guessing.\n",
        result.adjusted_rand,
        result.mutual_information,
        result.neighbourhood_overlap,
        result.n_found,
        result.n_planted,
    )
}

/// `1234567` as `1 234 567`, so a size is read at a glance.
fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(cells: usize, millis: &[u64]) -> Timing {
        Timing {
            stage: "step 1".into(),
            cells,
            samples: Samples::from(
                millis
                    .iter()
                    .map(|&v| Duration::from_millis(v))
                    .collect::<Vec<_>>(),
            ),
            peak_rss: Some(2 * 1024 * 1024),
        }
    }

    #[test]
    fn the_table_has_one_row_per_measurement() {
        let table = timings(&[timing(1000, &[10, 12, 11]), timing(2000, &[20, 22, 21])]);
        assert_eq!(table.lines().count(), 4, "header, rule and two rows");
        assert!(
            table.contains("2 MB") || table.contains("2.0 MB"),
            "{table}"
        );
    }

    #[test]
    fn the_per_cell_time_divides_by_the_cell_count() {
        let row = timing(1000, &[10, 10, 10]);
        assert_eq!(row.per_cell(), Duration::from_micros(10));
    }

    #[test]
    fn an_empty_cohort_has_no_per_cell_time() {
        assert_eq!(timing(0, &[10]).per_cell(), Duration::ZERO);
    }

    #[test]
    fn large_numbers_are_grouped() {
        assert_eq!(thousands(1), "1");
        assert_eq!(thousands(1000), "1\u{202f}000");
        assert_eq!(thousands(1234567), "1\u{202f}234\u{202f}567");
    }

    #[test]
    fn a_clean_golden_run_says_so_without_a_list() {
        let report = golden(&[]);
        assert!(report.contains("no drift"));
        assert!(!report.contains("- "));
    }

    #[test]
    fn a_dirty_golden_run_names_every_stage() {
        let report = golden(&["`edges` differs".into(), "`assortativity` differs".into()]);
        assert!(report.contains("2 stage(s) moved"));
        assert!(report.contains("`edges`"));
        assert!(report.contains("`assortativity`"));
        // A reader must be told how to accept a change they intended.
        assert!(report.contains("--update"));
    }

    #[test]
    fn the_recovery_report_states_both_counts() {
        let report = recovery(&crate::levels::Recovery {
            adjusted_rand: 0.87,
            mutual_information: 0.79,
            neighbourhood_overlap: 0.42,
            n_found: 5,
            n_planted: 5,
        });
        assert!(report.contains("0.870"));
        assert!(report.contains("Niches found | 5"));
    }
}
