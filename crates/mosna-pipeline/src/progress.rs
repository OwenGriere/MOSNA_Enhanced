//! Progress reporting.
//!
//! The GUI drives its status line and progress bar by parsing two kinds of line
//! on the analysis process's stdout. Keeping that behind a trait means the same
//! pipeline code runs silently under test and chatty under the GUI.

/// Sink for progress and status messages.
pub trait Progress: Sync {
    /// A status message, shown in the GUI's status bar.
    fn info(&self, message: &str);

    /// Progress through a step.
    fn step(&self, current: usize, total: usize, description: &str);

    /// Name the run directory this analysis is writing into.
    ///
    /// # Why this is part of the protocol
    ///
    /// A run's numbers are handed out by the register while the analysis runs,
    /// so nothing outside the process knows them until it has finished — and a
    /// sweep has to link each configuration it launched to the results it
    /// produced. Reading it back out of a log line meant for humans, or
    /// guessing at the newest directory, are both ways of being wrong on the
    /// day two runs overlap.
    ///
    /// Defaulted to nothing, so a reporter that does not care — every one under
    /// test — need not mention it.
    fn run_directory(&self, _name: &str) {}
}

/// Emits the `[QT_INFO]` and `[QT_PROGRESS]` lines the GUI parses.
///
/// The exact spelling is the protocol; see `mosna_io::progress`.
pub struct StdoutProgress;

impl Progress for StdoutProgress {
    fn info(&self, message: &str) {
        mosna_io::emit_qt_info(message);
    }

    fn step(&self, current: usize, total: usize, description: &str) {
        mosna_io::emit_qt_progress(current, total, description);
    }

    fn run_directory(&self, name: &str) {
        println!("[QT_RUN] {name}");
    }
}

/// Discards everything. Used by the tests, where stdout is noise.
pub struct SilentProgress;

impl Progress for SilentProgress {
    fn info(&self, _message: &str) {}
    fn step(&self, _current: usize, _total: usize, _description: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_silent_reporter_accepts_everything() {
        SilentProgress.info("ignored");
        SilentProgress.step(1, 2, "ignored");
        SilentProgress.run_directory("1-1-1");
    }

    /// The trait must stay object-safe: the pipelines take `&dyn Progress` so
    /// the GUI and the tests can supply different implementations.
    #[test]
    fn the_trait_is_object_safe() {
        let reporters: Vec<&dyn Progress> = vec![&SilentProgress, &StdoutProgress];
        assert_eq!(reporters.len(), 2);
    }
}
