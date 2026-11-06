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

    /// How long one stage of the computation took: the algorithm alone, not
    /// the checking, reading, writing or drawing around it. `None` when the
    /// stage was not computed because its result was read from the cache.
    ///
    /// A stage that runs once per sample is reported once, with the sum over
    /// the samples — see `mosna_core::timing`. One that runs per sample in a
    /// per-sample niche analysis is reported each time, and the reader adds
    /// them up.
    ///
    /// Defaulted to nothing, like [`Progress::run_directory`].
    fn timing(&self, _stage: &str, _seconds: Option<f64>) {}

    /// How long one stage took for one sample, or for the whole cohort when
    /// the stage runs once over all of them (the reduction and the clustering
    /// of a pooled niche analysis). `None` when it was read from the cache.
    ///
    /// The detail behind [`Progress::timing`], which keeps reporting the sums:
    /// the command line's `--benchmark` writes these as one row per network.
    /// Called from the worker threads, in whatever order the samples finish.
    ///
    /// Defaulted to nothing, like [`Progress::run_directory`].
    fn sample_timing(&self, _subject: Subject<'_>, _stage: &str, _seconds: Option<f64>) {}
}

/// What a [`Progress::sample_timing`] measurement is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject<'a> {
    /// One network.
    Sample(&'a mosna_io::SampleId),
    /// The cohort as a whole, for a stage that is not run sample by sample.
    Cohort,
}

/// Report every sample's share of `clock` under `stage`.
pub(crate) fn report_samples(
    progress: &dyn Progress,
    stage: &str,
    clock: &mosna_core::timing::ComputeClock,
) {
    for (id, seconds) in clock.by_sample() {
        progress.sample_timing(Subject::Sample(&id), stage, Some(seconds));
    }
}

/// The `[QT_TIME]` line: `[QT_TIME] seconds=1.234567 stage=Delaunay triangulation`,
/// or `seconds=cached` for a stage read from the cache. The stage comes last
/// because it may hold spaces.
pub fn timing_line(stage: &str, seconds: Option<f64>) -> String {
    match seconds {
        Some(seconds) => format!("[QT_TIME] seconds={seconds:.6} stage={stage}"),
        None => format!("[QT_TIME] seconds=cached stage={stage}"),
    }
}

/// The names the computing stages are reported under, as `[QT_TIME]` spells
/// them: one place, so a benchmark reading them can rely on the spelling.
pub mod stage {
    pub const DELAUNAY: &str = "Delaunay triangulation";
    pub const RECONNECTION: &str = "Reconnection of isolated cells";
    pub const ASSORTATIVITY: &str = "Assortativity and permutations";
    pub const AGGREGATION: &str = "Aggregation";
    pub const REDUCTION: &str = "Reduction";
    pub const CLUSTERING: &str = "Clustering";
    /// One sample from start to finish: reading, computing, queueing its
    /// figures and writing its files — the wall-clock time, I/O included.
    pub const SAMPLE_TOTAL: &str = "Sample total (with I/O)";
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

    fn timing(&self, stage: &str, seconds: Option<f64>) {
        println!("{}", timing_line(stage, seconds));
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
    fn the_timing_line_names_the_stage_last() {
        assert_eq!(
            timing_line("Delaunay triangulation", Some(1.5)),
            "[QT_TIME] seconds=1.500000 stage=Delaunay triangulation"
        );
        assert_eq!(
            timing_line("Reduction", None),
            "[QT_TIME] seconds=cached stage=Reduction"
        );
    }

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
