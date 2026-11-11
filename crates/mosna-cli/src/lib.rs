//! Command line interface of the MOSNA analyses.
//!
//! Mirrors the Python entry points, which the GUI launches as sub-processes:
//!
//! ```text
//! python -m package.tysserand_network --file <cfg> --working_dir <dir>
//! python -m package.assortativity     --file <cfg> --working_dir <dir>
//! python -m package.niche_analysis    --file <cfg> --working_dir <dir>
//! python -m package.clear_temporary                --working_dir <dir>
//! ```
//!
//! become
//!
//! ```text
//! mosna tysserand-network --file <cfg> --working_dir <dir>
//! mosna assortativity     --file <cfg> --working_dir <dir>
//! mosna niche-analysis    --file <cfg> --working_dir <dir>
//! mosna clear-temporary                --working_dir <dir>
//! ```
//!
//! and one command the Python never had, for the same reason it never had the
//! figures it reports on:
//!
//! ```text
//! mosna generate-report                --working_dir <dir>
//! ```
//!
//! The flag names are kept exactly — including `--working_dir` with its
//! underscore, which is not the usual CLI spelling but is what the Python
//! `argparse` declares and therefore what any existing script passes.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod benchmark;
pub mod sensitivity;
pub mod timings;

use mosna_pipeline::{
    assortativity, clear_temporary, compare_sweep, generate_report, niche_analysis_in,
    tysserand_network,
};
use mosna_xy::Figures;

/// Spatial network construction and analysis for spatial omics.
#[derive(Debug, Parser)]
#[command(name = "mosna", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Also write the execution time as a CSV, in `<working_dir>/benchmark/`.
    ///
    /// One row per patient/sample — per network — and one column per
    /// important stage, in seconds: the algorithms alone, plus each sample from
    /// start to finish with its I/O. A last `cohort` row holds the stages run
    /// once over the whole cohort and the whole command's time. An empty cell
    /// is a stage not computed for that row, read from the cache for instance.
    /// Accepted by every command, before or after its name.
    #[arg(long = "benchmark", global = true)]
    pub benchmark: bool,
}

impl Cli {
    /// Parse an argument vector, returning clap's error rather than exiting.
    ///
    /// The binary lets clap exit on its own; the tests need the error.
    pub fn parse_from<I, T>(argv: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        <Self as Parser>::try_parse_from(argv)
    }
}

/// The four analyses, plus the two operations on a finished directory.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Step 1 — reconstruct a spatial network for every sample.
    #[command(name = "tysserand-network")]
    TysserandNetwork {
        /// Path to `configuration.yaml`.
        #[arg(long = "file")]
        file: PathBuf,
        /// Working directory, as chosen in the interface.
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
        /// Formats each figure is written in, comma-separated (`png`, `svg`).
        ///
        /// Figures are never written as HTML: the only HTML a run produces is
        /// `report.html`, and only `generate-report` writes it.
        #[arg(long = "figure-formats", default_value = "png")]
        figure_formats: String,
        /// Write the tables, but set the figures aside instead of drawing them.
        ///
        /// Drawing is a large part of a run's time on a big cohort. What is
        /// set aside is every figure's full description, so `render-figures`
        /// draws them later without computing anything again.
        #[arg(long = "no-figures")]
        no_figures: bool,
    },

    /// Step 2 — z-scored assortativity and mixing matrices.
    #[command(name = "assortativity")]
    Assortativity {
        #[arg(long = "file")]
        file: PathBuf,
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
        /// Formats each figure is written in, comma-separated (`png`, `svg`).
        ///
        /// Figures are never written as HTML: the only HTML a run produces is
        /// `report.html`, and only `generate-report` writes it.
        #[arg(long = "figure-formats", default_value = "png")]
        figure_formats: String,
        /// Write the tables, but set the figures aside instead of drawing them.
        ///
        /// Drawing is a large part of a run's time on a big cohort. What is
        /// set aside is every figure's full description, so `render-figures`
        /// draws them later without computing anything again.
        #[arg(long = "no-figures")]
        no_figures: bool,
    },

    /// Step 3 — identify spatial niches.
    #[command(name = "niche-analysis")]
    NicheAnalysis {
        #[arg(long = "file")]
        file: PathBuf,
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
        /// Write the results into this sub-directory of `Niche_Analysis`.
        ///
        /// A sweep of two hundred runs would otherwise bury the handful started
        /// by hand. Only the results move: the register and the intermediate
        /// files stay where they are, because a run's numbers name the label
        /// column written into every nodes file and two registers would hand
        /// the same number to two different partitions.
        #[arg(long = "results-in")]
        results_in: Option<String>,
        /// Formats each figure is written in, comma-separated (`png`, `svg`).
        ///
        /// Figures are never written as HTML: the only HTML a run produces is
        /// `report.html`, and only `generate-report` writes it.
        #[arg(long = "figure-formats", default_value = "png")]
        figure_formats: String,
        /// Write the tables, but set the figures aside instead of drawing them.
        ///
        /// Drawing is a large part of a run's time on a big cohort. What is
        /// set aside is every figure's full description, so `render-figures`
        /// draws them later without computing anything again.
        #[arg(long = "no-figures")]
        no_figures: bool,
    },

    /// Compare the runs of a sensitivity sweep and draw the three figures.
    ///
    /// Takes the configuration for the two columns that say how the cohort is
    /// split, and nothing else: the runs it compares are the directories that
    /// are there, and their labels are in the network files. It can therefore
    /// be pointed at a sweep from last week.
    #[command(name = "compare-sweep")]
    CompareSweep {
        #[arg(long = "file")]
        file: PathBuf,
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
        /// The sub-directory of `Niche_Analysis` holding the sweep.
        #[arg(long = "results-in", default_value = "sensitivity_analysis")]
        results_in: String,
        #[arg(long = "figure-formats", default_value = "png")]
        figure_formats: String,
    },

    /// Run a sensitivity analysis from the plan the interface wrote.
    ///
    /// The plan — `sensitivity_plan.log`, written by "Write the plan (log)" on
    /// the sensitivity screen — says what will be done and holds everything
    /// needed to do it: the runs, in order, and the configuration they all
    /// start from. Each run is an ordinary niche analysis into the same results
    /// directory as the interface's Start button, followed by the comparison
    /// table and figures. Progress is shown as two bars: the grid, and the run
    /// in progress.
    #[command(name = "sensitivity-analysis")]
    SensitivityAnalysis {
        /// The plan file to run.
        #[arg(long = "log")]
        log: PathBuf,
        /// Run in this working directory instead of the one the plan names —
        /// for a plan written on one machine and run on another.
        #[arg(long = "working_dir")]
        working_dir: Option<PathBuf>,
        /// Print what the plan will do, and stop.
        #[arg(long = "plan-only")]
        plan_only: bool,
    },

    /// Draw every figure set aside by a run started with `--no-figures`.
    ///
    /// Computes nothing: it reads the descriptions those runs left in the
    /// working directory and hands them to the renderer, oldest first.
    #[command(name = "render-figures")]
    RenderFigures {
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
        #[arg(long = "figure-formats", default_value = "png")]
        figure_formats: String,
    },

    /// Remove the intermediate network files.
    #[command(name = "clear-temporary")]
    ClearTemporary {
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
    },

    /// Collect everything in the working directory into one HTML report.
    ///
    /// Takes no configuration: the report describes the directory as it stands,
    /// which is what lets it be run on results copied off a cluster.
    #[command(name = "generate-report")]
    GenerateReport {
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
    },
}

impl Command {
    /// The sub-command name as it appears on the command line.
    pub fn name(&self) -> &'static str {
        match self {
            Command::TysserandNetwork { .. } => "tysserand-network",
            Command::Assortativity { .. } => "assortativity",
            Command::NicheAnalysis { .. } => "niche-analysis",
            Command::CompareSweep { .. } => "compare-sweep",
            Command::SensitivityAnalysis { .. } => "sensitivity-analysis",
            Command::RenderFigures { .. } => "render-figures",
            Command::ClearTemporary { .. } => "clear-temporary",
            Command::GenerateReport { .. } => "generate-report",
        }
    }

    /// The working directory the command was given, if it was.
    pub fn working_dir(&self) -> Option<&std::path::Path> {
        match self {
            Command::TysserandNetwork { working_dir, .. }
            | Command::Assortativity { working_dir, .. }
            | Command::NicheAnalysis { working_dir, .. }
            | Command::CompareSweep { working_dir, .. }
            | Command::RenderFigures { working_dir, .. }
            | Command::ClearTemporary { working_dir }
            | Command::GenerateReport { working_dir } => Some(working_dir),
            Command::SensitivityAnalysis { working_dir, .. } => working_dir.as_deref(),
        }
    }

    /// Whether the figures are to be set aside rather than drawn.
    pub fn no_figures(&self) -> bool {
        match self {
            Command::TysserandNetwork { no_figures, .. }
            | Command::Assortativity { no_figures, .. }
            | Command::NicheAnalysis { no_figures, .. } => *no_figures,
            _ => false,
        }
    }

    /// The figure formats this command was asked for.
    ///
    /// The two that draw nothing have none, and are never asked. `html` is
    /// dropped whatever was asked: the report is the only HTML written.
    pub fn figure_formats(&self) -> Vec<String> {
        let spelled = match self {
            Command::TysserandNetwork { figure_formats, .. }
            | Command::Assortativity { figure_formats, .. }
            | Command::NicheAnalysis { figure_formats, .. }
            | Command::CompareSweep { figure_formats, .. }
            | Command::RenderFigures { figure_formats, .. } => figure_formats.as_str(),
            // The plan carries its own formats.
            Command::ClearTemporary { .. }
            | Command::GenerateReport { .. }
            | Command::SensitivityAnalysis { .. } => "",
        };
        spelled
            .split(',')
            .map(str::trim)
            .filter(|format| !format.is_empty())
            .filter(|format| !format.eq_ignore_ascii_case("html"))
            .map(str::to_string)
            .collect()
    }
}

/// Read a configuration file, with its input directories made absolute.
///
/// `--working_dir` is where results go and nothing else: the nodes and the
/// network directories are read where the configuration says, as in the
/// interface. A relative one is taken relative to the configuration file
/// itself — not to the working directory, which merged the two, and not to
/// the shell's current directory, which would make the same command read
/// different data depending on where it was typed.
pub fn load_config(file: &std::path::Path) -> anyhow::Result<mosna_config::RawConfig> {
    let mut config = mosna_config::get_config(file)?;
    let file = std::path::absolute(file)?;
    let base = file.parent().unwrap_or(std::path::Path::new("/"));
    config.resolve_input_paths(base);
    Ok(config)
}

/// Execute a parsed command.
///
/// Progress goes to stdout in the `[QT_INFO]` / `[QT_PROGRESS]` form the
/// interface parses, and figures are drawn by the Python `xy` package.
///
/// # Two phases, and why
///
/// The analysis queues its figures as it goes and they are all drawn at the
/// end, in one pass. One interpreter is started per run rather than one per
/// figure — a cohort of two hundred samples would otherwise spend more time
/// starting Python than drawing — and the renderer reports its own progress
/// through the same protocol, so the interface's bar keeps moving.
pub fn run(cli: Cli) -> anyhow::Result<()> {
    // The same lines on stdout as ever, for the interface; the computing time
    // of each stage is also kept, and given back as a table at the end.
    let timings = timings::Timings::new();
    let benchmark = cli.benchmark.then(benchmark::Benchmark::new);
    let progress = timings::TimedProgress {
        timings: &timings,
        benchmark: benchmark.as_ref(),
    };
    let started = std::time::Instant::now();
    // The directory the results went to: the sensitivity analysis may only
    // know it once it has read its plan.
    let mut results_dir = cli.command.working_dir().map(std::path::Path::to_path_buf);
    let formats = cli.command.figure_formats();
    let no_figures = cli.command.no_figures();
    // Drawn now, or set aside for `render-figures`.
    let finish = |figures: &Figures| -> anyhow::Result<()> {
        if no_figures {
            figures.defer().map(|_| ())
        } else {
            figures.render()
        }
    };
    let sink = |working_dir: &std::path::Path| {
        let borrowed: Vec<&str> = formats.iter().map(String::as_str).collect();
        Figures::with_renderer(
            working_dir,
            mosna_xy::renderer::Renderer::detect().formats(&borrowed),
        )
    };

    match cli.command {
        Command::TysserandNetwork {
            ref file,
            ref working_dir,
            ..
        } => {
            let config = load_config(file)?;
            let figures = sink(working_dir);
            tysserand_network(&config, working_dir, &progress, &figures)?;
            finish(&figures)?;
        }
        Command::Assortativity {
            ref file,
            ref working_dir,
            ..
        } => {
            let config = load_config(file)?;
            let figures = sink(working_dir);
            assortativity(&config, working_dir, &progress, &figures)?;
            finish(&figures)?;
        }
        Command::NicheAnalysis {
            ref file,
            ref working_dir,
            ref results_in,
            ..
        } => {
            let config = load_config(file)?;
            let figures = sink(working_dir);
            niche_analysis_in(
                &config,
                working_dir,
                results_in.as_deref(),
                &progress,
                &figures,
            )?;
            finish(&figures)?;
        }
        Command::CompareSweep {
            ref file,
            ref working_dir,
            ref results_in,
            ..
        } => {
            // Through the typed view rather than by reading the YAML here: the
            // two columns mean what `NicheAnalysisConfig` says they mean, and a
            // second reading of the same keys is a second thing to keep in step.
            let config = load_config(file)?;
            let settings = mosna_config::NicheAnalysisConfig::from_raw(&config)?;

            let figures = sink(working_dir);
            compare_sweep(
                working_dir,
                results_in,
                &settings.patient_column,
                settings.sample_column.as_deref(),
                &progress,
                &figures,
            )?;
            figures.render()?;
        }
        Command::RenderFigures {
            ref working_dir, ..
        } => {
            let borrowed: Vec<&str> = formats.iter().map(String::as_str).collect();
            let renderer = mosna_xy::renderer::Renderer::detect().formats(&borrowed);
            mosna_xy::render_deferred(working_dir, &renderer)?;
        }
        Command::SensitivityAnalysis {
            ref log,
            ref working_dir,
            plan_only,
        } => {
            results_dir =
                sensitivity::run_plan(log, working_dir.as_deref(), plan_only, benchmark.as_ref())?;
        }
        Command::ClearTemporary { ref working_dir } => {
            clear_temporary(working_dir, &progress)?;
        }
        Command::GenerateReport { ref working_dir } => {
            generate_report(working_dir, &progress)?;
        }
    }

    // Only once the command has succeeded: a failed run's times describe
    // half of a computation.
    if let Some(summary) = timings.summary() {
        println!();
        print!("{summary}");
    }
    if let (Some(benchmark), Some(directory)) = (&benchmark, &results_dir) {
        benchmark.command_total(started.elapsed().as_secs_f64());
        if let Some(path) = benchmark.write(directory, cli.command.name())? {
            println!("Benchmark written to {}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The commands that take no configuration: they act on a directory that
    /// already exists, so asking for a YAML would be asking for something the
    /// user does not need to have.
    const WITHOUT_CONFIG: [&str; 3] = ["clear-temporary", "generate-report", "render-figures"];

    #[test]
    fn every_command_reports_its_own_name() {
        let names = [
            "tysserand-network",
            "assortativity",
            "niche-analysis",
            "compare-sweep",
            "clear-temporary",
            "generate-report",
            "render-figures",
        ];
        for name in names {
            let argv: Vec<&str> = if WITHOUT_CONFIG.contains(&name) {
                vec!["mosna", name, "--working_dir", "/w"]
            } else {
                vec!["mosna", name, "--file", "c.yaml", "--working_dir", "/w"]
            };
            let cli = Cli::parse_from(argv).unwrap();
            assert_eq!(cli.command.name(), name);
        }
    }

    /// The report is built from the directory, not from the configuration: it
    /// must be possible to report on a folder copied off a cluster, whose YAML
    /// is somewhere else entirely.
    #[test]
    fn the_report_needs_only_a_directory() {
        assert!(Cli::parse_from(["mosna", "generate-report", "--working_dir", "/w"]).is_ok());
        assert!(Cli::parse_from([
            "mosna",
            "generate-report",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w"
        ])
        .is_err());
    }

    /// The sensitivity analysis is started from its plan alone: the plan holds
    /// the configuration, and names the working directory.
    #[test]
    fn the_sensitivity_analysis_needs_only_its_plan() {
        let cli = Cli::parse_from(["mosna", "sensitivity-analysis", "--log", "plan.log"]).unwrap();
        assert_eq!(cli.command.name(), "sensitivity-analysis");
        assert!(Cli::parse_from([
            "mosna",
            "sensitivity-analysis",
            "--log",
            "plan.log",
            "--working_dir",
            "/w",
            "--plan-only"
        ])
        .is_ok());
        assert!(Cli::parse_from(["mosna", "sensitivity-analysis"]).is_err());
    }

    #[test]
    fn the_working_dir_flag_keeps_its_underscore() {
        // `--working-dir` would be the usual spelling, and is what clap would
        // derive by default; the Python declares `--working_dir`, so any
        // existing script or launcher passes that.
        assert!(Cli::parse_from(["mosna", "clear-temporary", "--working_dir", "/w"]).is_ok());
        assert!(Cli::parse_from(["mosna", "clear-temporary", "--working-dir", "/w"]).is_err());
    }

    /// `--benchmark` is accepted by every command, after its name or before.
    #[test]
    fn every_command_takes_the_benchmark_flag() {
        for name in [
            "tysserand-network",
            "assortativity",
            "niche-analysis",
            "compare-sweep",
            "clear-temporary",
            "generate-report",
            "render-figures",
        ] {
            let mut argv: Vec<&str> = if WITHOUT_CONFIG.contains(&name) {
                vec!["mosna", name, "--working_dir", "/w"]
            } else {
                vec!["mosna", name, "--file", "c.yaml", "--working_dir", "/w"]
            };
            assert!(!Cli::parse_from(argv.clone()).unwrap().benchmark);
            argv.push("--benchmark");
            assert!(Cli::parse_from(argv).unwrap().benchmark, "`{name}`");
        }
        let before = Cli::parse_from([
            "mosna",
            "--benchmark",
            "sensitivity-analysis",
            "--log",
            "plan.yaml",
        ])
        .unwrap();
        assert!(before.benchmark);
    }

    #[test]
    fn the_command_line_definition_is_internally_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    // -----------------------------------------------------------------------
    // Choosing what the figures are written as
    // -----------------------------------------------------------------------

    /// Every figure is written twice, as a PNG and as an interactive HTML. One
    /// niche run is about 5 MB of them, so a sweep of two hundred runs is a
    /// gigabyte — and the HTML, which is a third of it, is for inspecting one
    /// run rather than for comparing many.
    ///
    /// The renderer has always been able to write one format; nothing could ask
    /// it to.
    #[test]
    fn an_analysis_can_be_asked_for_one_figure_format() {
        let cli = Cli::parse_from([
            "mosna",
            "niche-analysis",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w",
            "--figure-formats",
            "png",
        ])
        .unwrap();
        assert_eq!(cli.command.figure_formats(), vec!["png"]);
    }

    /// PNG only, by default: figures are never written as HTML.
    #[test]
    fn only_png_is_written_unless_told_otherwise() {
        let cli = Cli::parse_from([
            "mosna",
            "niche-analysis",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w",
        ])
        .unwrap();
        assert_eq!(cli.command.figure_formats(), vec!["png"]);
    }

    /// Asking for HTML figures is ignored: only the report is HTML.
    #[test]
    fn html_figures_are_never_asked_for() {
        let cli = Cli::parse_from([
            "mosna",
            "niche-analysis",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w",
            "--figure-formats",
            "png,HTML,svg",
        ])
        .unwrap();
        assert_eq!(cli.command.figure_formats(), vec!["png", "svg"]);
    }

    /// Several, comma-separated, the way the renderer already spells them.
    #[test]
    fn several_formats_are_comma_separated() {
        let cli = Cli::parse_from([
            "mosna",
            "tysserand-network",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w",
            "--figure-formats",
            "png,svg",
        ])
        .unwrap();
        assert_eq!(cli.command.figure_formats(), vec!["png", "svg"]);
    }

    /// The commands that draw nothing do not take it.
    #[test]
    fn the_commands_that_draw_nothing_do_not_offer_the_flag() {
        for name in ["clear-temporary", "generate-report"] {
            assert!(
                Cli::parse_from([
                    "mosna",
                    name,
                    "--working_dir",
                    "/w",
                    "--figure-formats",
                    "png"
                ])
                .is_err(),
                "`{name}` accepted --figure-formats"
            );
        }
    }

    /// The three analyses can be run without drawing, and say so.
    #[test]
    fn an_analysis_can_set_its_figures_aside() {
        for name in ["tysserand-network", "assortativity", "niche-analysis"] {
            let cli = Cli::parse_from([
                "mosna",
                name,
                "--file",
                "c.yaml",
                "--working_dir",
                "/w",
                "--no-figures",
            ])
            .unwrap();
            assert!(cli.command.no_figures(), "`{name}` ignored --no-figures");

            let cli = Cli::parse_from(["mosna", name, "--file", "c.yaml", "--working_dir", "/w"])
                .unwrap();
            assert!(
                !cli.command.no_figures(),
                "`{name}` skips its figures by default"
            );
        }
    }

    /// Drawing what was set aside needs a directory and nothing else: the
    /// descriptions carry everything the figures need.
    #[test]
    fn the_figures_set_aside_are_drawn_from_a_directory_alone() {
        let cli = Cli::parse_from(["mosna", "render-figures", "--working_dir", "/w"]).unwrap();
        assert_eq!(cli.command.name(), "render-figures");
        assert_eq!(cli.command.figure_formats(), vec!["png"]);
        assert!(Cli::parse_from([
            "mosna",
            "render-figures",
            "--working_dir",
            "/w",
            "--no-figures"
        ])
        .is_err());
    }

    /// A sweep writes its results apart from the runs started by hand, so two
    /// hundred of them do not bury the handful that were chosen.
    #[test]
    fn an_analysis_can_be_told_where_to_put_its_results() {
        let cli = Cli::parse_from([
            "mosna",
            "niche-analysis",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w",
            "--results-in",
            "sensitivity_analysis",
        ])
        .unwrap();
        match cli.command {
            Command::NicheAnalysis { results_in, .. } => {
                assert_eq!(results_in.as_deref(), Some("sensitivity_analysis"));
            }
            other => panic!("expected a niche analysis, got {other:?}"),
        }
    }

    /// Without it the results go where they always have.
    #[test]
    fn results_go_to_the_usual_place_unless_told_otherwise() {
        let cli = Cli::parse_from([
            "mosna",
            "niche-analysis",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w",
        ])
        .unwrap();
        match cli.command {
            Command::NicheAnalysis { results_in, .. } => assert_eq!(results_in, None),
            other => panic!("expected a niche analysis, got {other:?}"),
        }
    }

    /// And the steps that write nothing to `Niche_Analysis` do not offer it.
    #[test]
    fn the_other_steps_do_not_offer_a_results_directory() {
        for name in ["tysserand-network", "assortativity"] {
            assert!(
                Cli::parse_from([
                    "mosna",
                    name,
                    "--file",
                    "c.yaml",
                    "--working_dir",
                    "/w",
                    "--results-in",
                    "x"
                ])
                .is_err(),
                "`{name}` accepted --results-in"
            );
        }
    }
}
