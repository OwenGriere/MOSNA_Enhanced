//! The computing time of a command, given back at the end of it.
//!
//! The pipelines report how long each stage's algorithm took — not the
//! checking, reading, writing or drawing around it — through
//! [`Progress::timing`]. The interface shows that in its Queue tab; on the
//! command line it is collected here and printed as a table once the command
//! has finished, so a benchmark run from a terminal or a cluster job gets the
//! same numbers.

use std::sync::Mutex;

use mosna_pipeline::{Progress, StdoutProgress};

/// One stage's computing time, added up over every time it was reported.
#[derive(Debug, Clone, PartialEq)]
pub struct StageTime {
    pub stage: String,
    pub seconds: f64,
    /// How many times it was computed.
    pub computed: usize,
    /// How many times it was read from the cache instead.
    pub cached: usize,
}

/// The stages of a command, in the order they were first reported.
#[derive(Debug, Default)]
pub struct Timings {
    stages: Mutex<Vec<StageTime>>,
}

impl Timings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one report.
    pub fn add(&self, stage: &str, seconds: Option<f64>) {
        let Ok(mut stages) = self.stages.lock() else {
            return;
        };
        let index = match stages.iter().position(|s| s.stage == stage) {
            Some(index) => index,
            None => {
                stages.push(StageTime {
                    stage: stage.to_string(),
                    seconds: 0.0,
                    computed: 0,
                    cached: 0,
                });
                stages.len() - 1
            }
        };
        let entry = &mut stages[index];
        match seconds {
            Some(seconds) => {
                entry.seconds += seconds;
                entry.computed += 1;
            }
            None => entry.cached += 1,
        }
    }

    /// Everything recorded so far.
    pub fn stages(&self) -> Vec<StageTime> {
        self.stages.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// The table printed at the end of a command, or `None` if nothing was
    /// timed — the commands that compute nothing print nothing.
    pub fn summary(&self) -> Option<String> {
        summary(&self.stages())
    }
}

/// The table, from stages already added up.
pub fn summary(stages: &[StageTime]) -> Option<String> {
    if stages.is_empty() {
        return None;
    }
    let width = stages
        .iter()
        .map(|s| s.stage.chars().count())
        .max()
        .unwrap_or(0)
        .max("total".len());

    let mut out = String::from(
        "Compute time — the algorithms alone, without checking, reading or writing \
         files, nor drawing figures; summed over the samples where they run in parallel:\n",
    );
    let mut total = 0.0;
    for stage in stages {
        let time = if stage.computed > 0 {
            format!("{:>10.3} s", stage.seconds)
        } else {
            format!("{:>12}", "cached")
        };
        let mut line = format!("  {:<width$}  {time}", stage.stage);
        if stage.computed > 1 || stage.cached > 0 {
            let mut counts = format!("{}× computed", stage.computed);
            if stage.cached > 0 {
                counts.push_str(&format!(", {}× cached", stage.cached));
            }
            line.push_str(&format!("   ({counts})"));
        }
        out.push_str(&line);
        out.push('\n');
        total += stage.seconds;
    }
    out.push_str(&format!("  {:<width$}  {total:>10.3} s\n", "total"));
    Some(out)
}

/// The interface's reporter, keeping the timings as they go past.
///
/// Everything still reaches stdout exactly as before — the interface launches
/// these same commands and reads the same lines.
pub struct TimedProgress<'a> {
    pub timings: &'a Timings,
}

impl Progress for TimedProgress<'_> {
    fn info(&self, message: &str) {
        StdoutProgress.info(message);
    }

    fn step(&self, current: usize, total: usize, description: &str) {
        StdoutProgress.step(current, total, description);
    }

    fn run_directory(&self, name: &str) {
        StdoutProgress.run_directory(name);
    }

    fn timing(&self, stage: &str, seconds: Option<f64>) {
        StdoutProgress.timing(stage, seconds);
        self.timings.add(stage, seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_timed_prints_nothing() {
        assert_eq!(Timings::new().summary(), None);
    }

    #[test]
    fn the_table_adds_up_each_stage_and_the_whole() {
        let timings = Timings::new();
        timings.add("Aggregation", Some(1.0));
        timings.add("Reduction", None);
        timings.add("Aggregation", Some(0.5));
        timings.add("Clustering", Some(0.25));

        let text = timings.summary().unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("Compute time"));
        assert_eq!(lines[1], "  Aggregation       1.500 s   (2× computed)");
        assert_eq!(
            lines[2],
            "  Reduction          cached   (0× computed, 1× cached)"
        );
        assert_eq!(lines[3], "  Clustering        0.250 s");
        assert_eq!(lines[4], "  total             1.750 s");
    }

    /// The reporter still writes what the interface reads.
    #[test]
    fn the_timed_reporter_keeps_what_it_forwards() {
        let timings = Timings::new();
        let progress = TimedProgress { timings: &timings };
        progress.timing("Delaunay triangulation", Some(0.5));
        assert_eq!(timings.stages()[0].seconds, 0.5);
    }
}
