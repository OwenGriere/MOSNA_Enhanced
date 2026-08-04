//! The bench, from the command line.
//!
//! ```text
//! cargo run --release -p mosna-bench -- golden          # level 1: check
//! cargo run --release -p mosna-bench -- golden --update # level 1: re-record
//! cargo run --release -p mosna-bench -- reproduce       # level 2
//! cargo run --release -p mosna-bench -- recover         # level 3
//! cargo run --release -p mosna-bench -- perf            # timings
//! cargo run --release -p mosna-bench -- all             # everything
//! ```
//!
//! Always in `--release`. A debug build measures the debug build.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use mosna_bench::cohort::CohortSpec;
use mosna_bench::fingerprint::Fingerprint;
use mosna_bench::levels;
use mosna_bench::report::{self, Timing};
use mosna_bench::timing::{measure, peak_rss_bytes};

#[derive(Debug, Parser)]
#[command(
    name = "mosna-bench",
    version,
    about = "Performance and reproducibility bench for MOSNA"
)]
struct Cli {
    /// Cells per sample.
    #[arg(long, default_value_t = 10_000)]
    cells: usize,

    /// Samples in the cohort.
    #[arg(long, default_value_t = 4)]
    samples: usize,

    /// Seed of the synthetic tissue. Changing it changes the data, and so the
    /// golden reference.
    #[arg(long, default_value_t = 20_260_804)]
    seed: u64,

    /// Where the golden references live.
    #[arg(long)]
    golden_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Level 1: check the deterministic core against the recorded reference.
    Golden {
        /// Re-record instead of checking. Use when a change was intended.
        #[arg(long)]
        update: bool,
        /// Relative tolerance on the numeric stages.
        #[arg(long, default_value_t = 1e-9)]
        tolerance: f64,
    },
    /// Level 2: the seeded stages must not depend on the thread count.
    Reproduce,
    /// Level 3: score the niches found against the niches planted.
    Recover,
    /// Time the stages across cohort sizes.
    Perf {
        /// Runs per measurement. The median is reported, so three is a floor.
        #[arg(long, default_value_t = 5)]
        repeats: usize,
        /// Cohort sizes to sweep, in cells per sample.
        #[arg(long, value_delimiter = ',', default_values_t = [2_000usize, 10_000, 50_000])]
        sizes: Vec<usize>,
    },
    /// Every level, then the timings.
    All,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let spec = CohortSpec {
        n_samples: cli.samples,
        cells_per_sample: cli.cells,
        n_phenotypes: 12,
        n_niches: 5,
        seed: cli.seed,
    };

    let golden_dir = cli
        .golden_dir
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden"));

    match cli.command {
        Command::Golden { update, tolerance } => run_golden(&spec, &golden_dir, update, tolerance)?,
        Command::Reproduce => run_reproduce(&spec),
        Command::Recover => run_recover(&spec),
        Command::Perf { repeats, sizes } => run_perf(&spec, repeats, &sizes),
        Command::All => {
            run_golden(&spec, &golden_dir, false, 1e-9)?;
            run_reproduce(&spec);
            run_recover(&spec);
            run_perf(&spec, 5, &[2_000, 10_000]);
        }
    }
    Ok(())
}

/// The reference file for a given cohort shape.
///
/// The shape is in the name: a reference recorded on ten thousand cells says
/// nothing about a run on fifty thousand, and silently comparing the two would
/// be worse than having no reference at all.
fn reference_path(spec: &CohortSpec, dir: &Path) -> PathBuf {
    dir.join(format!(
        "level1_{}samples_{}cells_seed{}.json",
        spec.n_samples, spec.cells_per_sample, spec.seed
    ))
}

fn run_golden(spec: &CohortSpec, dir: &Path, update: bool, tolerance: f64) -> anyhow::Result<()> {
    let path = reference_path(spec, dir);
    let fingerprint = levels::level_1_golden(spec);

    if update {
        fingerprint.save(&path)?;
        println!("Recorded {}", path.display());
        return Ok(());
    }

    let Ok(reference) = Fingerprint::load(&path) else {
        fingerprint.save(&path)?;
        println!(
            "No reference for this cohort shape; recorded {}.\n\
             Commit it, and this run becomes the baseline.",
            path.display()
        );
        return Ok(());
    };

    let differences = fingerprint.differences(&reference, tolerance);
    print!("{}", report::golden(&differences));
    if !differences.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

fn run_reproduce(spec: &CohortSpec) {
    let result = levels::level_2_reproducibility(spec);
    print!("{}", report::reproducibility(&result));
    if !result.is_reproducible() {
        std::process::exit(1);
    }
}

fn run_recover(spec: &CohortSpec) {
    print!("{}", report::recovery(&levels::level_3_recovery(spec)));
}

fn run_perf(spec: &CohortSpec, repeats: usize, sizes: &[usize]) {
    let mut rows = Vec::new();

    for &cells in sizes {
        let spec = CohortSpec {
            cells_per_sample: cells,
            ..spec.clone()
        };

        // Timed separately, because they answer different questions: the first
        // is geometry and aggregation over the whole cohort, the second is the
        // projection and the clustering of one sample.
        rows.push(Timing {
            stage: "geometry + NAS + assortativity".into(),
            cells: cells * spec.n_samples,
            samples: measure(repeats, || {
                std::hint::black_box(levels::level_1_golden(&spec));
            }),
            peak_rss: peak_rss_bytes(),
        });

        rows.push(Timing {
            stage: "UMAP + GMM".into(),
            cells,
            samples: measure(repeats.min(3), || {
                std::hint::black_box(levels::level_3_recovery(&spec));
            }),
            peak_rss: peak_rss_bytes(),
        });
    }

    println!("{}", report::timings(&rows));
    println!(
        "Peak RSS is the high-water mark of the whole process, so it only ever \
         grows across a sweep; read the largest row."
    );
}
