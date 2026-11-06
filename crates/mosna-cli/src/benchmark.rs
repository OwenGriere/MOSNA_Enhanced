//! `--benchmark`: the execution time of a command, as a CSV, one row per network.
//!
//! The pipelines report how long each important stage took for each sample
//! (`Progress::sample_timing`), and for the cohort when a stage runs once over
//! all of them. This collects those reports and writes them, once the command
//! has succeeded, to `<working_dir>/benchmark/<command>_<date>.csv`:
//!
//! ```text
//! run,patient,sample,Delaunay triangulation,Reconnection of isolated cells,Sample total (with I/O),Command total (wall clock)
//! ,1,1,0.012000,0.001000,0.040000,
//! ,1,2,0.010000,0.001000,0.035000,
//! ,cohort,,,,,1.250000
//! ```
//!
//! - one row per patient/sample — that is, per network — then one `cohort`
//!   row for the stages that are not run sample by sample (the reduction and
//!   the clustering of a pooled niche analysis) and for the whole command;
//! - one column per stage, in seconds. The stages are the algorithms alone,
//!   without the reading, writing or drawing around them, except `Sample total
//!   (with I/O)`, which is one sample from start to finish, and `Command total
//!   (wall clock)`, which is the whole command;
//! - an empty cell is a stage that was not computed for that row: read from the
//!   cache, or not part of that kind of row. Empty rather than a word, so the
//!   columns stay numeric when the file is loaded;
//! - a `run` column names the `Niche_Analysis` run directory the row belongs to
//!   when there is one: a niche analysis that does both the aggregated and the
//!   per-sample analysis has two, and a sensitivity analysis one per run.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use mosna_io::SampleId;
use mosna_pipeline::Subject;

/// The directory under the working directory the files are written into.
pub const DIRECTORY: &str = "benchmark";

/// The column holding the whole command's time.
pub const COMMAND_TOTAL: &str = "Command total (wall clock)";

/// What a row is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Who {
    // Declared first so that, sorted, the samples come before the cohort.
    Sample(SampleId),
    Cohort,
}

/// One measurement: seconds, or read from the cache.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cell {
    Seconds(f64),
    Cached,
}

#[derive(Debug, Clone)]
struct Row {
    run: Option<String>,
    who: Who,
    cells: Vec<(String, Cell)>,
}

#[derive(Debug, Default)]
struct State {
    /// The run directory the reports now coming in belong to.
    run: Option<String>,
    /// Every stage seen, in the order first reported.
    stages: Vec<String>,
    rows: Vec<Row>,
}

/// The measurements of one command.
#[derive(Debug, Default)]
pub struct Benchmark {
    state: Mutex<State>,
}

impl Benchmark {
    pub fn new() -> Self {
        Self::default()
    }

    /// The analysis named the run directory it writes into; what follows
    /// belongs to it.
    pub fn run_directory(&self, name: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.run = Some(name.to_string());
        }
    }

    /// Record one report. Times add up when the same stage is reported twice
    /// for the same row; a cached report never hides a computed one.
    pub fn record(&self, subject: Subject<'_>, stage: &str, seconds: Option<f64>) {
        let who = match subject {
            Subject::Sample(id) => Who::Sample(id.clone()),
            Subject::Cohort => Who::Cohort,
        };
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let run = state.run.clone();
        self.put(&mut state, run, who, stage, seconds);
    }

    /// The whole command's time, on the cohort row outside any run.
    pub fn command_total(&self, seconds: f64) {
        if let Ok(mut state) = self.state.lock() {
            self.put(&mut state, None, Who::Cohort, COMMAND_TOTAL, Some(seconds));
        }
    }

    fn put(
        &self,
        state: &mut State,
        run: Option<String>,
        who: Who,
        stage: &str,
        seconds: Option<f64>,
    ) {
        if !state.stages.iter().any(|s| s == stage) {
            state.stages.push(stage.to_string());
        }
        let index = match state
            .rows
            .iter()
            .position(|row| row.run == run && row.who == who)
        {
            Some(index) => index,
            None => {
                state.rows.push(Row {
                    run,
                    who,
                    cells: Vec::new(),
                });
                state.rows.len() - 1
            }
        };
        let cells = &mut state.rows[index].cells;
        let new = match seconds {
            Some(seconds) => Cell::Seconds(seconds),
            None => Cell::Cached,
        };
        match cells.iter_mut().find(|(s, _)| s == stage) {
            None => cells.push((stage.to_string(), new)),
            Some((_, cell)) => {
                *cell = match (*cell, new) {
                    (Cell::Seconds(a), Cell::Seconds(b)) => Cell::Seconds(a + b),
                    (Cell::Seconds(a), Cell::Cached) | (Cell::Cached, Cell::Seconds(a)) => {
                        Cell::Seconds(a)
                    }
                    (Cell::Cached, Cell::Cached) => Cell::Cached,
                }
            }
        }
    }

    /// The table, or `None` when nothing was measured.
    pub fn to_csv(&self) -> Option<String> {
        let state = self.state.lock().ok()?;
        if state.rows.is_empty() {
            return None;
        }

        // The stages in the order they were reported, the two totals last.
        let totals = [mosna_pipeline::stage::SAMPLE_TOTAL, COMMAND_TOTAL];
        let mut stages: Vec<&str> = state
            .stages
            .iter()
            .map(String::as_str)
            .filter(|s| !totals.contains(s))
            .collect();
        stages.extend(
            totals
                .iter()
                .filter(|t| state.stages.iter().any(|s| s == *t)),
        );

        // Runs in the order they started, then samples in order, cohort last.
        let run_order = |run: &Option<String>| {
            state
                .rows
                .iter()
                .position(|row| &row.run == run)
                .unwrap_or(usize::MAX)
        };
        let mut rows: Vec<&Row> = state.rows.iter().collect();
        rows.sort_by(|a, b| {
            // The row outside any run — the command's own — goes at the very end.
            (a.run.is_none(), run_order(&a.run), &a.who).cmp(&(
                b.run.is_none(),
                run_order(&b.run),
                &b.who,
            ))
        });

        let with_runs = state.rows.iter().any(|row| row.run.is_some());
        let mut header: Vec<String> = Vec::new();
        if with_runs {
            header.push("run".to_string());
        }
        header.extend(["patient".to_string(), "sample".to_string()]);
        header.extend(stages.iter().map(|s| s.to_string()));

        let mut out = csv_line(&header);
        for row in rows {
            let mut fields: Vec<String> = Vec::new();
            if with_runs {
                fields.push(row.run.clone().unwrap_or_default());
            }
            match &row.who {
                Who::Sample(id) => {
                    fields.push(id.patient.clone());
                    fields.push(id.sample.clone().unwrap_or_default());
                }
                Who::Cohort => {
                    fields.push("cohort".to_string());
                    fields.push(String::new());
                }
            }
            for stage in &stages {
                let cell = row.cells.iter().find(|(s, _)| s == stage).map(|(_, c)| *c);
                fields.push(match cell {
                    Some(Cell::Seconds(seconds)) => format!("{seconds:.6}"),
                    Some(Cell::Cached) | None => String::new(),
                });
            }
            out.push_str(&csv_line(&fields));
        }
        Some(out)
    }

    /// Write the table under `<working_dir>/benchmark/`, and say where.
    ///
    /// Nothing is written when nothing was measured.
    pub fn write(&self, working_dir: &Path, command: &str) -> anyhow::Result<Option<PathBuf>> {
        let Some(text) = self.to_csv() else {
            return Ok(None);
        };
        let directory = working_dir.join(DIRECTORY);
        std::fs::create_dir_all(&directory)?;
        let stamp = mosna_config::clock::file_stamp(std::time::SystemTime::now());
        // Two commands finishing in the same second keep both files.
        let mut path = directory.join(format!("{command}_{stamp}.csv"));
        let mut n = 2;
        while path.exists() {
            path = directory.join(format!("{command}_{stamp}_{n}.csv"));
            n += 1;
        }
        std::fs::write(&path, text)?;
        Ok(Some(path))
    }
}

/// One CSV line, quoting the fields that need it.
fn csv_line(fields: &[String]) -> String {
    let quoted: Vec<String> = fields
        .iter()
        .map(|field| {
            if field.contains([',', '"', '\n']) {
                format!("\"{}\"", field.replace('"', "\"\""))
            } else {
                field.clone()
            }
        })
        .collect();
    format!("{}\n", quoted.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(patient: &str, sample: &str) -> SampleId {
        SampleId::with_sample(patient, sample)
    }

    #[test]
    fn nothing_measured_writes_nothing() {
        assert_eq!(Benchmark::new().to_csv(), None);
    }

    /// One row per network, one column per stage, the totals last and the
    /// cohort row at the end.
    #[test]
    fn one_row_per_network_and_one_column_per_stage() {
        let bench = Benchmark::new();
        let (b, a) = (id("2", "1"), id("1", "1"));
        // Reported out of order, as parallel samples finish.
        bench.record(Subject::Sample(&b), "Delaunay triangulation", Some(0.5));
        bench.record(Subject::Sample(&a), "Sample total (with I/O)", Some(2.0));
        bench.record(Subject::Sample(&a), "Delaunay triangulation", Some(0.25));
        bench.command_total(3.0);

        let text = bench.to_csv().unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines,
            [
                "patient,sample,Delaunay triangulation,Sample total (with I/O),Command total (wall clock)",
                "1,1,0.250000,2.000000,",
                "2,1,0.500000,,",
                "cohort,,,,3.000000",
            ]
        );
    }

    /// A stage that runs over the whole cohort is on the cohort row.
    #[test]
    fn a_stage_of_the_whole_cohort_is_on_its_own_row() {
        let bench = Benchmark::new();
        let a = id("1", "1");
        bench.record(Subject::Sample(&a), "Aggregation", Some(1.0));
        bench.record(Subject::Cohort, "Clustering", Some(4.0));
        let text = bench.to_csv().unwrap();
        assert!(text.ends_with("cohort,,,4.000000\n"), "{text}");
    }

    /// A cached stage is an empty cell, so the column stays numeric.
    #[test]
    fn a_cached_stage_is_an_empty_cell() {
        let bench = Benchmark::new();
        bench.record(Subject::Cohort, "Reduction", None);
        let text = bench.to_csv().unwrap();
        assert_eq!(text.lines().nth(1), Some("cohort,,"));
    }

    /// Each run of a niche analysis gets its own rows, named in a `run` column.
    #[test]
    fn rows_are_kept_apart_by_run() {
        let bench = Benchmark::new();
        let a = id("1", "1");
        bench.run_directory("1-1-1");
        bench.record(Subject::Sample(&a), "Aggregation", Some(1.0));
        bench.run_directory("1-1-2");
        bench.record(Subject::Sample(&a), "Aggregation", Some(2.0));
        bench.command_total(5.0);

        let text = bench.to_csv().unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "run,patient,sample,Aggregation,Command total (wall clock)"
        );
        assert_eq!(lines[1], "1-1-1,1,1,1.000000,");
        assert_eq!(lines[2], "1-1-2,1,1,2.000000,");
        assert_eq!(lines[3], ",cohort,,,5.000000");
    }

    /// Patients without samples leave the sample column empty.
    #[test]
    fn one_level_data_leaves_the_sample_empty() {
        let bench = Benchmark::new();
        let a = SampleId::patient_only("7");
        bench.record(
            Subject::Sample(&a),
            "Assortativity and permutations",
            Some(1.5),
        );
        assert_eq!(bench.to_csv().unwrap().lines().nth(1), Some("7,,1.500000"));
    }

    #[test]
    fn the_file_goes_under_the_benchmark_directory() {
        let dir = tempfile::tempdir().unwrap();
        let bench = Benchmark::new();
        bench.command_total(1.0);
        let path = bench.write(dir.path(), "assortativity").unwrap().unwrap();
        assert_eq!(path.parent().unwrap(), dir.path().join("benchmark"));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("assortativity_") && name.ends_with(".csv"),
            "{name}"
        );

        // A second one in the same second does not overwrite the first.
        let again = bench.write(dir.path(), "assortativity").unwrap().unwrap();
        assert_ne!(path, again);
    }
}
