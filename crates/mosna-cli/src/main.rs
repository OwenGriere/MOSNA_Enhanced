//! Binary entry point.

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = mosna_cli::Cli::parse();

    match mosna_cli::run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // The interface reads the last non-progress line of stderr to build
            // its error dialog, so the message goes there, unadorned.
            eprintln!("{error}");
            for cause in error.chain().skip(1) {
                eprintln!("  caused by: {cause}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
