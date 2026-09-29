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

use mosna_pipeline::{
    assortativity, clear_temporary, compare_sweep, generate_report, niche_analysis_in,
    tysserand_network, StdoutProgress,
};
use mosna_xy::Figures;

/// Spatial network construction and analysis for spatial omics.
#[derive(Debug, Parser)]
#[command(name = "mosna", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
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
        /// Formats each figure is written in, comma-separated.
        ///
        /// Every figure is written as a PNG and as an interactive HTML, which
        /// is about 5 MB per niche run. Sweeping a grid of parameters turns
        /// that into gigabytes, and the HTML — a third of it — is for
        /// inspecting one run rather than for comparing many. `--figure-formats
        /// png` halves the output; the HTML can be redrawn later from the
        /// embedding and the cached partition.
        #[arg(long = "figure-formats", default_value = "png,html")]
        figure_formats: String,
    },

    /// Step 2 — z-scored assortativity and mixing matrices.
    #[command(name = "assortativity")]
    Assortativity {
        #[arg(long = "file")]
        file: PathBuf,
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
        /// Formats each figure is written in, comma-separated.
        ///
        /// Every figure is written as a PNG and as an interactive HTML, which
        /// is about 5 MB per niche run. Sweeping a grid of parameters turns
        /// that into gigabytes, and the HTML — a third of it — is for
        /// inspecting one run rather than for comparing many. `--figure-formats
        /// png` halves the output; the HTML can be redrawn later from the
        /// embedding and the cached partition.
        #[arg(long = "figure-formats", default_value = "png,html")]
        figure_formats: String,
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
        /// Formats each figure is written in, comma-separated.
        ///
        /// Every figure is written as a PNG and as an interactive HTML, which
        /// is about 5 MB per niche run. Sweeping a grid of parameters turns
        /// that into gigabytes, and the HTML — a third of it — is for
        /// inspecting one run rather than for comparing many. `--figure-formats
        /// png` halves the output; the HTML can be redrawn later from the
        /// embedding and the cached partition.
        #[arg(long = "figure-formats", default_value = "png,html")]
        figure_formats: String,
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
        #[arg(long = "figure-formats", default_value = "png,html")]
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
            Command::ClearTemporary { .. } => "clear-temporary",
            Command::GenerateReport { .. } => "generate-report",
        }
    }

    /// The figure formats this command was asked for.
    ///
    /// The two that draw nothing have none, and are never asked.
    pub fn figure_formats(&self) -> Vec<String> {
        let spelled = match self {
            Command::TysserandNetwork { figure_formats, .. }
            | Command::Assortativity { figure_formats, .. }
            | Command::NicheAnalysis { figure_formats, .. }
            | Command::CompareSweep { figure_formats, .. } => figure_formats.as_str(),
            Command::ClearTemporary { .. } | Command::GenerateReport { .. } => "",
        };
        spelled
            .split(',')
            .map(str::trim)
            .filter(|format| !format.is_empty())
            .map(str::to_string)
            .collect()
    }
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
    let progress = StdoutProgress;
    let formats = cli.command.figure_formats();
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
            let config = mosna_config::get_config(file)?;
            let figures = sink(working_dir);
            tysserand_network(&config, working_dir, &progress, &figures)?;
            figures.render()?;
        }
        Command::Assortativity {
            ref file,
            ref working_dir,
            ..
        } => {
            let config = mosna_config::get_config(file)?;
            let figures = sink(working_dir);
            assortativity(&config, working_dir, &progress, &figures)?;
            figures.render()?;
        }
        Command::NicheAnalysis {
            ref file,
            ref working_dir,
            ref results_in,
            ..
        } => {
            let config = mosna_config::get_config(file)?;
            let figures = sink(working_dir);
            niche_analysis_in(
                &config,
                working_dir,
                results_in.as_deref(),
                &progress,
                &figures,
            )?;
            figures.render()?;
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
            let config = mosna_config::get_config(file)?;
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
        Command::ClearTemporary { ref working_dir } => {
            clear_temporary(working_dir, &progress)?;
        }
        Command::GenerateReport { ref working_dir } => {
            generate_report(working_dir, &progress)?;
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
    const WITHOUT_CONFIG: [&str; 2] = ["clear-temporary", "generate-report"];

    #[test]
    fn every_command_reports_its_own_name() {
        let names = [
            "tysserand-network",
            "assortativity",
            "niche-analysis",
            "compare-sweep",
            "clear-temporary",
            "generate-report",
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

    #[test]
    fn the_working_dir_flag_keeps_its_underscore() {
        // `--working-dir` would be the usual spelling, and is what clap would
        // derive by default; the Python declares `--working_dir`, so any
        // existing script or launcher passes that.
        assert!(Cli::parse_from(["mosna", "clear-temporary", "--working_dir", "/w"]).is_ok());
        assert!(Cli::parse_from(["mosna", "clear-temporary", "--working-dir", "/w"]).is_err());
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

    /// Both, by default — which is what every run has always produced.
    #[test]
    fn both_formats_are_written_unless_told_otherwise() {
        let cli = Cli::parse_from([
            "mosna",
            "niche-analysis",
            "--file",
            "c.yaml",
            "--working_dir",
            "/w",
        ])
        .unwrap();
        assert_eq!(cli.command.figure_formats(), vec!["png", "html"]);
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
        for name in WITHOUT_CONFIG {
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
