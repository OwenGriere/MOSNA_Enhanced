//! The HTML report.
//!
//! One button, one file: it reads the output directory as it stands and writes
//! a page that presents every figure in it, in the shape of the directory
//! itself. Nothing is recomputed and no analysis is run — a report can be made
//! at any point, including halfway through a cohort, and made again afterwards.

pub mod clock;
pub mod html;
pub mod layout;
pub mod png;
pub mod subject;
pub mod tree;

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{PipelineError, Result};
use crate::progress::Progress;

/// The name of the report, at the root of the working directory.
///
/// Fixed, and next to the results rather than under them: a report the user has
/// to go looking for is a report that does not get read. Being fixed is also
/// what lets it be excluded from its own listing, and overwritten in place
/// rather than accumulating.
pub const FILE_NAME: &str = "report.html";

/// Write the report, and return where it went.
pub fn generate_report(working_dir: &Path, progress: &dyn Progress) -> Result<PathBuf> {
    generate_report_at(working_dir, SystemTime::now(), progress)
}

/// The same, with the clock passed in so the tests can pin what it writes.
pub fn generate_report_at(
    working_dir: &Path,
    now: SystemTime,
    progress: &dyn Progress,
) -> Result<PathBuf> {
    progress.info("[PROCESS] Reading the output directory");
    progress.step(0, 2, "[PROCESS] Generate report");

    let output = tree::scan(working_dir, FILE_NAME);
    let figures: usize = output
        .galleries
        .iter()
        .map(|gallery| gallery.figures.len())
        .sum();

    progress.step(1, 2, "[PROCESS] Generate report");

    let page = html::render(&html::Page {
        working_dir,
        generated: &clock::stamp(now),
        output: &output,
    });

    let path = working_dir.join(FILE_NAME);
    std::fs::write(&path, page).map_err(|source| PipelineError::Write {
        path: path.clone(),
        source,
    })?;

    progress.info(&format!(
        "[INFO] Report of {figures} figures written to {}",
        path.display()
    ));
    progress.step(2, 2, "[PROCESS] Generate report");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;

    fn touch(path: PathBuf) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn the_report_lands_beside_the_results() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Tysserand_Network/net_1.png"));

        let path = generate_report(dir.path(), &SilentProgress).unwrap();

        assert_eq!(path, dir.path().join("report.html"));
        assert!(path.is_file());
    }

    #[test]
    fn the_figures_that_are_there_are_in_it() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Assortativity/abundance.png"));
        touch(dir.path().join("Assortativity/abundance.html"));

        let path = generate_report(dir.path(), &SilentProgress).unwrap();
        let page = std::fs::read_to_string(path).unwrap();

        assert!(page.contains("abundance"));
        assert!(page.contains("Assortativity/abundance.html"));
        assert!(page.contains("1 figure"), "the count is wrong");
    }

    /// Pressed twice, it must replace the report rather than grow one — and the
    /// second report must not describe the first.
    #[test]
    fn generating_twice_replaces_the_report() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Tysserand_Network/net_1.png"));

        generate_report(dir.path(), &SilentProgress).unwrap();
        let first = std::fs::read_to_string(dir.path().join("report.html")).unwrap();
        generate_report(dir.path(), &SilentProgress).unwrap();
        let second = std::fs::read_to_string(dir.path().join("report.html")).unwrap();

        assert!(
            !second.contains("report.html"),
            "the report describes itself"
        );
        assert_eq!(
            first.matches("<figure>").count(),
            second.matches("<figure>").count(),
            "the second report is not the same shape as the first"
        );
    }

    /// The most likely first press of the button: before any analysis has run.
    #[test]
    fn an_empty_working_directory_still_produces_a_report() {
        let dir = tempfile::tempdir().unwrap();
        let path = generate_report(dir.path(), &SilentProgress).unwrap();

        let page = std::fs::read_to_string(path).unwrap();
        assert!(page.to_lowercase().contains("no figure"));
    }

    /// The date is in the page, and it is the one the caller's clock gave.
    #[test]
    fn the_report_says_when_it_was_made() {
        let dir = tempfile::tempdir().unwrap();
        let at = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_787_069_100);

        let path = generate_report_at(dir.path(), at, &SilentProgress).unwrap();
        let page = std::fs::read_to_string(path).unwrap();

        assert!(
            page.contains("2026-08-18 16:05 UTC"),
            "no date in the report"
        );
    }

    /// A working directory that is not there is worth a message naming it, not
    /// an `Os { code: 2 }`.
    #[test]
    fn a_missing_working_directory_is_reported_by_name() {
        let error = generate_report(Path::new("/nowhere/at/all"), &SilentProgress).unwrap_err();
        let message = error.to_string();

        assert!(message.contains("/nowhere/at/all"), "{message}");
    }

    /// The report is written after the scan, so the scan cannot see it — the
    /// listing of a first report and of a tenth are the same.
    #[test]
    fn the_listing_never_includes_the_report() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path().join("Assortativity/net_stat.csv"));

        generate_report(dir.path(), &SilentProgress).unwrap();
        generate_report(dir.path(), &SilentProgress).unwrap();
        let page = std::fs::read_to_string(dir.path().join("report.html")).unwrap();

        assert!(page.contains("net_stat.csv"));
        assert_eq!(page.matches("report.html").count(), 0);
    }
}
