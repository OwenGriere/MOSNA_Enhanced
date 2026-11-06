//! `mosna sensitivity-analysis --log <plan>`: run a sensitivity analysis from
//! the plan the interface wrote, in a terminal, with progress bars.
//!
//! # The same analysis as the interface's Start button
//!
//! The plan holds the combinations in the order the interface would run them,
//! the configuration they all start from, the sub-section they are written
//! into, the normalisation and the figure formats. Each run is the ordinary
//! `niche_analysis_in`, into the same results directory, so the register, the
//! caches and the comparison table are the ones the interface produces — a grid
//! started here can be looked at there, and the other way round.
//!
//! What differs is only how the runs are driven. The interface launches one
//! sub-process per run because it must stay responsive and be able to kill a
//! run; here nothing else is waiting, so the runs are called in this process,
//! one after the other, and Ctrl-C stops the whole thing.
//!
//! # The bars
//!
//! Two, one above the other: the grid — runs done, time elapsed, time left —
//! and the run in progress, fed by the same `Progress` reports the interface
//! turns into its own bar. The figure renderer is run with its output captured,
//! because its progress lines are meant for the interface and would otherwise
//! be printed across the bars.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde_yaml::Value;

use mosna_config::model::sweep_plan::{Recomputes, SweepPlan};
use mosna_pipeline::{compare_sweep, niche_analysis_in, Progress};
use mosna_xy::renderer::Renderer;
use mosna_xy::Figures;

/// The comparison table a finished sweep leaves beside its runs; the name the
/// interface gives it.
pub const SENSITIVITY_CSV: &str = "sensitivity.csv";

/// What became of one run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// The run directory, once the analysis named it.
    pub run: Option<String>,
    pub succeeded: bool,
    pub niches: Option<u64>,
    pub largest: Option<u64>,
    pub graph_components: Option<u64>,
    pub failure: Option<String>,
}

impl Outcome {
    /// Fill in what a finished run recorded about itself in its `run.json`.
    ///
    /// The same fields the interface reads into its table.
    fn absorb(&mut self, record: &serde_json::Value) {
        let result = &record["result"];
        self.niches = result["niches"].as_u64();
        self.largest = result["sizes"]
            .as_array()
            .and_then(|sizes| sizes.iter().filter_map(serde_json::Value::as_u64).max());
        self.graph_components = result["graph_components"].as_u64();
    }
}

/// Feeds a run's progress reports to the lower bar, and keeps the name of the
/// run directory the analysis claims.
struct RunBar {
    bar: ProgressBar,
    run: Mutex<Option<String>>,
    /// The computing time of each stage, as the run reported it.
    timings: Mutex<Vec<(String, Option<f64>)>>,
}

impl RunBar {
    fn new(bar: &ProgressBar) -> Self {
        Self {
            bar: bar.clone(),
            run: Mutex::new(None),
            timings: Mutex::new(Vec::new()),
        }
    }

    /// `aggregation cached · reduction 2.104 s · clustering 0.311 s`.
    fn timings(&self) -> String {
        let Ok(timings) = self.timings.lock() else {
            return String::new();
        };
        timings
            .iter()
            .map(|(stage, seconds)| match seconds {
                Some(seconds) => format!("{} {seconds:.3} s", stage.to_lowercase()),
                None => format!("{} cached", stage.to_lowercase()),
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl Progress for RunBar {
    fn info(&self, message: &str) {
        self.bar.set_message(clean(message));
    }

    fn step(&self, current: usize, total: usize, description: &str) {
        self.bar.set_length(total.max(1) as u64);
        self.bar.set_position(current as u64);
        self.bar.set_message(clean(description));
    }

    fn run_directory(&self, name: &str) {
        if let Ok(mut run) = self.run.lock() {
            *run = Some(name.to_string());
        }
    }

    fn timing(&self, stage: &str, seconds: Option<f64>) {
        if let Ok(mut timings) = self.timings.lock() {
            timings.push((stage.to_string(), seconds));
        }
    }
}

/// A progress message without the `[INFO]`-style tags meant for the interface.
fn clean(message: &str) -> String {
    let mut text = message.trim();
    while let Some(rest) = text.strip_prefix('[') {
        match rest.find(']') {
            Some(end) => text = rest[end + 1..].trim_start(),
            None => break,
        }
    }
    text.lines().next().unwrap_or_default().to_string()
}

/// A value as a table cell shows it: as the interface's `Point::label` does.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(state) => state.to_string(),
        Value::Number(number) if number.is_f64() => format!("{:.6}", number.as_f64().unwrap())
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

/// One run's settings on a line: `order=1, resolution=0.005`.
fn label(settings: &[(String, Value)]) -> String {
    settings
        .iter()
        .map(|(key, value)| format!("{key}={}", plain(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The comparison table, in the interface's layout: everything the sweep
/// varied, then everything it found, one line per run.
pub fn to_csv(plan: &SweepPlan, outcomes: &[Outcome]) -> String {
    let escape = |field: &str| {
        if field.contains([',', '"', '\n']) {
            format!("\"{}\"", field.replace('"', "\"\""))
        } else {
            field.to_string()
        }
    };

    let mut out = String::from("run,status");
    for column in &plan.columns {
        out.push(',');
        out.push_str(&escape(column));
    }
    out.push_str(",niches,largest_niche,graph_components\n");

    for (run, outcome) in plan.runs.iter().zip(outcomes) {
        out.push_str(&escape(outcome.run.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(if outcome.succeeded { "done" } else { "failed" });
        for column in &plan.columns {
            out.push(',');
            out.push_str(&escape(&run.get(column).map(plain).unwrap_or_default()));
        }
        for value in [outcome.niches, outcome.largest, outcome.graph_components] {
            out.push(',');
            if let Some(value) = value {
                out.push_str(&value.to_string());
            }
        }
        out.push('\n');
    }
    out
}

/// The explanation at the top of a plan file, without its `#`.
pub fn explanation(text: &str) -> String {
    text.lines()
        .take_while(|line| line.starts_with('#'))
        .map(|line| {
            let line = line.trim_start_matches('#');
            line.strip_prefix(' ').unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Print one line of the run's story.
///
/// Above the bars in a terminal. Anywhere else — a batch job's log file, a
/// pipe — the bars are not drawn at all, and the lines are the only record of
/// how far the grid got, so they go to stdout as they are.
fn say(bars: &MultiProgress, interactive: bool, line: String) {
    if interactive {
        let _ = bars.println(line);
    } else {
        println!("{line}");
    }
}

fn style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template)
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("█▓░")
}

/// Run the plan at `log`.
///
/// `working_dir` replaces the one the plan names — for a plan written on one
/// machine and run on another. `plan_only` prints what would be done and stops.
pub fn run_plan(log: &Path, working_dir: Option<&Path>, plan_only: bool) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(log)
        .with_context(|| format!("cannot read the plan {}", log.display()))?;
    let plan = SweepPlan::parse(&text)?;

    if plan_only {
        println!("{}", explanation(&text));
        return Ok(());
    }

    let working_dir: PathBuf = working_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| plan.working_dir.clone());
    if !working_dir.is_dir() {
        bail!(
            "the working directory {} does not exist; pass --working_dir to run the plan elsewhere",
            working_dir.display()
        );
    }
    if plan.runs.is_empty() {
        bail!("the plan has no run");
    }

    let total = plan.runs.len();
    let recomputes = plan.recomputes();
    let (aggregations, projections, _) = plan.costs();
    let results = working_dir.join("Niche_Analysis").join(&plan.results_in);
    let formats: Vec<&str> = plan.figure_formats.iter().map(String::as_str).collect();

    println!("Sensitivity analysis — {}", log.display());
    println!(
        "  {total} run(s) · at most {aggregations} aggregation(s) and {projections} projection(s) to compute"
    );
    println!("  results in {}", results.display());
    println!();

    // Bars only where someone is watching: indicatif draws nothing elsewhere.
    let interactive = std::io::stderr().is_terminal();
    let bars = MultiProgress::new();
    let grid = bars.add(ProgressBar::new(total as u64));
    grid.set_style(style(
        "{prefix:.bold} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} runs · ETA {eta}  {msg}",
    ));
    grid.set_prefix("Sensitivity");
    let current = bars.add(ProgressBar::new(1));
    current.set_style(style(
        "  {spinner:.green} {bar:30.green/white} {pos:>5}/{len:5} {wide_msg}",
    ));
    current.enable_steady_tick(Duration::from_millis(120));

    let mut outcomes: Vec<Outcome> = Vec::with_capacity(total);
    // The computing time of every stage, over the whole grid.
    let grid_timings = crate::timings::Timings::new();
    for (index, run) in plan.runs.iter().enumerate() {
        let short = match recomputes[index] {
            Recomputes::Everything => "aggregation + projection + clustering",
            Recomputes::FromReduction => "projection + clustering",
            Recomputes::Clustering => "clustering",
        };
        grid.set_message(format!("run {}: {short}", index + 1));
        say(
            &bars,
            interactive,
            format!("▶ run {}/{total} — {}", index + 1, label(&run.settings)),
        );
        current.reset();
        current.set_length(1);
        current.set_message("starting");

        let reporter = RunBar::new(&current);
        let config = plan.configuration(index);
        let figures =
            Figures::with_renderer(&working_dir, Renderer::detect_captured().formats(&formats));
        let result = niche_analysis_in(
            &config,
            &working_dir,
            Some(&plan.results_in),
            &reporter,
            &figures,
        )
        .map_err(anyhow::Error::from)
        .and_then(|()| {
            current.set_message("drawing the figures");
            figures.render()
        });

        if let Ok(timings) = reporter.timings.lock() {
            for (stage, seconds) in timings.iter() {
                grid_timings.add(stage, *seconds);
            }
        }
        let mut outcome = Outcome {
            run: reporter.run.lock().ok().and_then(|run| run.clone()),
            ..Outcome::default()
        };
        match result {
            Ok(()) => {
                outcome.succeeded = true;
                if let Some(record) = outcome
                    .run
                    .as_deref()
                    .map(|run| results.join(run).join("run.json"))
                    .and_then(|path| std::fs::read_to_string(path).ok())
                    .and_then(|text| serde_json::from_str(&text).ok())
                {
                    outcome.absorb(&record);
                }
                say(
                    &bars,
                    interactive,
                    format!(
                        "  ✓ {} — {} niche(s) · compute: {}",
                        outcome.run.as_deref().unwrap_or("done"),
                        outcome
                            .niches
                            .map_or_else(|| "?".to_string(), |n| n.to_string()),
                        reporter.timings()
                    ),
                );
            }
            Err(error) => {
                let reason = error.to_string();
                say(&bars, interactive, format!("  ✗ failed — {reason}"));
                outcome.failure = Some(reason);
            }
        }
        outcomes.push(outcome);
        grid.inc(1);
    }

    // The table first: it is the deliverable, and it must exist even if the
    // comparison below fails.
    std::fs::create_dir_all(&results)
        .with_context(|| format!("cannot create {}", results.display()))?;
    let table = results.join(SENSITIVITY_CSV);
    std::fs::write(&table, to_csv(&plan, &outcomes))
        .with_context(|| format!("cannot write {}", table.display()))?;

    let succeeded = outcomes.iter().filter(|o| o.succeeded).count();
    if succeeded >= 2 {
        grid.set_message("comparing the runs");
        current.reset();
        current.set_message("comparing the runs");
        let reporter = RunBar::new(&current);
        let settings = mosna_config::NicheAnalysisConfig::from_raw(&plan.base)?;
        // Interactive copies too, as the interface asks for: the three
        // comparison figures are the answer the sweep was run for.
        let figures = Figures::with_renderer(
            &working_dir,
            Renderer::detect_captured().formats(&["png", "html"]),
        );
        let compared = compare_sweep(
            &working_dir,
            &plan.results_in,
            &settings.patient_column,
            settings.sample_column.as_deref(),
            &reporter,
            &figures,
        )
        .map_err(anyhow::Error::from)
        .and_then(|()| figures.render());
        if let Err(error) = compared {
            say(
                &bars,
                interactive,
                format!("  the runs could not be compared: {error}"),
            );
        }
    }

    current.finish_and_clear();
    grid.finish_with_message("done");

    let failed = total - succeeded;
    println!();
    println!(
        "{succeeded} of {total} run(s) succeeded — {}",
        table.display()
    );
    // Given back whatever the outcome: the runs that did finish were timed.
    if let Some(summary) = grid_timings.summary() {
        println!();
        print!("{summary}");
    }
    if failed > 0 {
        let first = outcomes
            .iter()
            .find_map(|o| o.failure.clone())
            .unwrap_or_default();
        bail!("{failed} of {total} run(s) failed; the first: {first}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosna_config::model::sweep_plan::PlannedRun;
    use mosna_config::RawConfig;

    fn plan() -> SweepPlan {
        SweepPlan {
            created: "now".to_string(),
            working_dir: PathBuf::from("/w"),
            results_in: "sensitivity_analysis".to_string(),
            sub_section: "Aggregated nodes".to_string(),
            normalize: None,
            figure_formats: vec!["png".to_string()],
            columns: vec!["order".to_string(), "resolution".to_string()],
            requested: 2,
            runs: vec![
                PlannedRun {
                    settings: vec![
                        ("order".to_string(), Value::from("1")),
                        ("resolution".to_string(), Value::from(0.0108)),
                    ],
                },
                PlannedRun {
                    settings: vec![
                        ("order".to_string(), Value::from("2")),
                        ("resolution".to_string(), Value::from(0.05)),
                    ],
                },
            ],
            base: RawConfig::from_yaml_str("Niche Analysis:\n  Aggregated nodes: {}\n").unwrap(),
        }
    }

    /// The table has the interface's layout, so the two produce the same file.
    #[test]
    fn the_table_has_the_interfaces_layout() {
        let outcomes = vec![
            Outcome {
                run: Some("1-1-1".to_string()),
                succeeded: true,
                niches: Some(6),
                largest: Some(120),
                graph_components: Some(1),
                failure: None,
            },
            Outcome {
                failure: Some("no".to_string()),
                ..Outcome::default()
            },
        ];
        assert_eq!(
            to_csv(&plan(), &outcomes),
            "run,status,order,resolution,niches,largest_niche,graph_components\n\
             1-1-1,done,1,0.0108,6,120,1\n\
             ,failed,2,0.05,,,\n"
        );
    }

    #[test]
    fn the_interfaces_tags_are_taken_off_the_messages() {
        assert_eq!(clean("[MULTI PROCESS] Processing file"), "Processing file");
        assert_eq!(
            clean("[INFO] Parameters are read correctly\n"),
            "Parameters are read correctly"
        );
        assert_eq!(clean("plain"), "plain");
    }

    #[test]
    fn a_run_is_labelled_by_its_settings() {
        assert_eq!(
            label(&plan().runs[0].settings),
            "order=1, resolution=0.0108"
        );
    }

    #[test]
    fn the_explanation_is_the_comment_block() {
        let text = "# Title\n#\n# body\nformat: x\n# not this\n";
        assert_eq!(explanation(text), "Title\n\nbody");
    }
}
