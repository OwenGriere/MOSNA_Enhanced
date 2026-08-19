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
    assortativity, clear_temporary, generate_report, niche_analysis, tysserand_network,
    StdoutProgress,
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
    },

    /// Step 2 — z-scored assortativity and mixing matrices.
    #[command(name = "assortativity")]
    Assortativity {
        #[arg(long = "file")]
        file: PathBuf,
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
    },

    /// Step 3 — identify spatial niches.
    #[command(name = "niche-analysis")]
    NicheAnalysis {
        #[arg(long = "file")]
        file: PathBuf,
        #[arg(long = "working_dir")]
        working_dir: PathBuf,
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
            Command::ClearTemporary { .. } => "clear-temporary",
            Command::GenerateReport { .. } => "generate-report",
        }
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

    match cli.command {
        Command::TysserandNetwork { file, working_dir } => {
            let config = mosna_config::get_config(&file)?;
            let figures = Figures::new(&working_dir);
            tysserand_network(&config, &working_dir, &progress, &figures)?;
            figures.render()?;
        }
        Command::Assortativity { file, working_dir } => {
            let config = mosna_config::get_config(&file)?;
            let figures = Figures::new(&working_dir);
            assortativity(&config, &working_dir, &progress, &figures)?;
            figures.render()?;
        }
        Command::NicheAnalysis { file, working_dir } => {
            let config = mosna_config::get_config(&file)?;
            let figures = Figures::new(&working_dir);
            niche_analysis(&config, &working_dir, &progress, &figures)?;
            figures.render()?;
        }
        Command::ClearTemporary { working_dir } => {
            clear_temporary(&working_dir, &progress)?;
        }
        Command::GenerateReport { working_dir } => {
            generate_report(&working_dir, &progress)?;
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
}
