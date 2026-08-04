//! Binary entry point of the installer.

use std::path::PathBuf;

use clap::Parser;

use mosna_install::{Installer, Sources};
use mosna_paths::{layout::Layout, Environment};

/// Install MOSNA into a prefix, or remove it again.
#[derive(Debug, Parser)]
#[command(name = "mosna-install", version, about, long_about = None)]
struct Cli {
    /// Where to install. Defaults to `~/.local`, which needs no root.
    #[arg(long)]
    prefix: Option<PathBuf>,

    /// Directory holding the built binaries. Defaults to `target/release`.
    #[arg(long)]
    build_dir: Option<PathBuf>,

    /// The configuration to ship as a starting point.
    #[arg(long)]
    config: Option<PathBuf>,

    /// The icon to install. Skipped when absent.
    #[arg(long)]
    icon: Option<PathBuf>,

    /// Remove a previous install instead of writing one.
    #[arg(long)]
    uninstall: bool,

    /// Report what would be done, without touching the disk.
    #[arg(long)]
    dry_run: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let environment = Environment::detect();

    let prefix = cli
        .prefix
        .or_else(|| Layout::default_prefix(&environment))
        .ok_or_else(|| {
            anyhow::anyhow!("no --prefix given and HOME is not set, so there is no default")
        })?;
    let layout = Layout::new(&prefix);

    let build_dir = cli
        .build_dir
        .unwrap_or_else(|| PathBuf::from("target/release"));
    let sources = Sources {
        analysis_binary: build_dir.join(mosna_paths::binary::ANALYSIS_FILE_NAME),
        interface_binary: build_dir.join(mosna_paths::binary::INTERFACE_FILE_NAME),
        config: cli
            .config
            .unwrap_or_else(|| PathBuf::from("CONFIG/configuration.yaml")),
        icon: cli.icon.filter(|path| path.is_file()),
    };

    let installer = Installer::new(layout.clone(), sources, environment.clone());

    let report = if cli.uninstall {
        installer.uninstall()?
    } else if cli.dry_run {
        installer.dry_run()?
    } else {
        installer.install()?
    };

    for line in &report {
        println!("{line}");
    }

    if cli.uninstall {
        if report.is_empty() {
            println!("nothing to remove under {}", prefix.display());
        }
        return Ok(());
    }
    if cli.dry_run {
        return Ok(());
    }

    println!();
    println!("MOSNA is installed under {}.", prefix.display());
    report_path_advice(&layout);
    println!(
        "Run `mosna-install --uninstall --prefix {}` to remove it.",
        prefix.display()
    );

    Ok(())
}

/// Tell the user if the install directory is not on their `PATH`.
///
/// `~/.local/bin` is on `PATH` on most distributions but not all, and an
/// install whose commands cannot be found looks like an install that failed.
fn report_path_advice(layout: &Layout) {
    let bin_dir = layout.bin_dir();
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|entry| entry == bin_dir))
        .unwrap_or(false);

    if on_path {
        println!("Run `mosna-gui` to start, or `mosna --help` for the command line.");
    } else {
        println!(
            "Note: {} is not on your PATH. Either add it,\n\
             or start the interface from its full path: {}",
            bin_dir.display(),
            layout.interface_binary().display()
        );
    }
}
