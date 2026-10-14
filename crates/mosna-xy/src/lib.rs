//! Figures, drawn by the Python `xy` package.
//!
//! The analyses hand their figures to [`mosna_pipeline::FigureSink`]. This
//! crate implements that seam by writing one *specification* per figure — the
//! values, the labels, the colours, the file to write — and then running
//! `python -m mosna_xy render` over the lot.
//!
//! # Why not draw them here
//!
//! Because the figures are read by people, and the questions they get asked
//! are "what is that outlier" and "which sample is that column". A static
//! image cannot answer either. `xy` produces an interactive chart and a PNG
//! from the same description, so the gallery keeps its images and the report
//! gets charts that can be zoomed and hovered.
//!
//! What did *not* move is the science. Which colour map, how a z-score is
//! normalised, in what order a dendrogram's leaves fall, what a title says:
//! all of that is decided here, in Rust, where the tests that pin it already
//! live. The renderer composes and exports; it does not decide.

pub mod dendrogram;
pub mod figures;
pub mod norm;
pub mod palette;
pub mod renderer;
pub mod spec;
pub mod table;

use std::path::{Path, PathBuf};

use mosna_core::niches::{NicheComposition, Normalize};
use mosna_io::SampleId;
use mosna_pipeline::FigureSink;

use crate::renderer::{Renderer, Run, Subprocess};
use crate::spec::{Queue, Spec};
use crate::table::Table;

/// Queues every figure an analysis produces, then has them drawn.
pub struct Figures<R: Run = Subprocess> {
    working_dir: PathBuf,
    queue: Queue,
    renderer: Renderer<R>,
}

impl Figures<Subprocess> {
    /// Figures for a run under `working_dir`, drawn by the interpreter this
    /// machine is set up to use.
    pub fn new(working_dir: &Path) -> Self {
        Self::with_renderer(working_dir, Renderer::detect())
    }
}

impl<R: Run> Figures<R> {
    pub fn with_renderer(working_dir: &Path, renderer: Renderer<R>) -> Self {
        Self {
            working_dir: working_dir.to_path_buf(),
            queue: Queue::new(working_dir),
            renderer,
        }
    }

    /// How many figures are waiting.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Draw everything queued, then remove the queue.
    ///
    /// An empty queue starts nothing: `clear-temporary`, and a run that found
    /// no samples, both end up here with nothing to say.
    ///
    /// # An install without a renderer is not a failed analysis
    ///
    /// MOSNA can be installed without the Python environment that draws the
    /// figures — it is the largest part of the install, and an analysis that
    /// writes its tables is worth having on a machine that cannot spare it. On
    /// such a machine every run would otherwise end in an error, having done
    /// all of its work and written all of its results.
    ///
    /// So a renderer that is not there is reported and stepped over, while a
    /// renderer that *is* there and fails is still an error: the first is a
    /// choice the user made at install time, the second is something broken.
    /// [`Renderer::check`] is what tells them apart — and it is asked only
    /// after a render has already failed, so an ordinary run still starts one
    /// interpreter and not two.
    pub fn render(&self) -> anyhow::Result<()> {
        if self.queue.is_empty() {
            return Ok(());
        }

        if let Err(failure) = self.renderer.render(self.queue.directory()) {
            if let Err(reason) = self.renderer.check() {
                // The queue is kept, so installing the renderer later leaves
                // the next run able to draw what this one could not.
                eprintln!(
                    "[QT_INFO] Figures not drawn: {reason}\n\
                     The analysis is complete and its tables are written. To \
                     draw the figures, install MOSNA again with INSTALLATION.exe \
                     (figures ticked) or install.sh (without --no-figures)."
                );
                return Ok(());
            }
            return Err(failure);
        }

        // Only once the figures exist. A failed render keeps its queue: that
        // is the exact input that produced the failure, and the alternative is
        // running the analysis again to get it back.
        if let Err(error) = self.queue.discard() {
            log_discard_failure(&error);
        }
        Ok(())
    }

    /// Keep everything queued for later instead of drawing it.
    ///
    /// For a run started *without plotting figures*: the tables are what was
    /// wanted now, and the figures are drawn afterwards by [`render_deferred`]
    /// — the "Plot all figures" button — from exactly these specifications,
    /// without the analysis being run again.
    ///
    /// The queue is moved, whole, under
    /// [`mosna_pipeline::figures::DEFERRED_FIGURE_DIRECTORY`], named after the
    /// moment it was set aside so that the batches are drawn in the order the
    /// runs ended: two runs that describe the same file then leave the later
    /// one's figure on disk, as they would have if each had drawn its own.
    ///
    /// Returns how many figures were set aside.
    pub fn defer(&self) -> anyhow::Result<usize> {
        let queued = self.queue.len();
        if queued == 0 || !self.queue.directory().is_dir() {
            return Ok(0);
        }

        let deferred = self
            .working_dir
            .join(mosna_pipeline::figures::DEFERRED_FIGURE_DIRECTORY);
        std::fs::create_dir_all(&deferred)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", deferred.display()))?;

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let name = self
            .queue
            .directory()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = deferred.join(format!("{stamp:020}-{name}"));

        std::fs::rename(self.queue.directory(), &target).map_err(|e| {
            anyhow::anyhow!("cannot set the figures aside in {}: {e}", target.display())
        })?;
        // The dot-directory the queues live in, if this was the last one.
        if let Some(parent) = self.queue.directory().parent() {
            let _ = std::fs::remove_dir(parent);
        }

        println!(
            "[QT_INFO] {queued} figures set aside, not drawn. \
             Press \"Plot all figures\" to draw them without computing again."
        );
        Ok(queued)
    }

    /// Queue one specification, turning a write failure into a pipeline error.
    fn push(&self, spec: Spec) -> mosna_pipeline::Result<()> {
        self.queue.push(spec).map(|_| ()).map_err(to_pipeline_error)
    }
}

impl<R: Run> FigureSink for Figures<R> {
    fn network(
        &self,
        sample: &SampleId,
        patient_column: &str,
        sample_column: Option<&str>,
        coords: &[[f64; 2]],
        pairs: &[(u32, u32)],
        labels: &[String],
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        self.push(figures::network::spec(
            sample,
            patient_column,
            sample_column,
            coords,
            pairs,
            labels,
            save_dir,
        ))
    }

    fn assortativity(
        &self,
        columns: &[String],
        rows: &[(String, Vec<f64>)],
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        let table = Table::new(columns, rows);

        self.push(figures::abundance::spec(&table, save_dir))?;
        for include_self_pairs in [true, false] {
            self.push(figures::heatmap::spec(&table, include_self_pairs, save_dir))?;
            self.push(figures::mean_std::spec(
                &table,
                include_self_pairs,
                save_dir,
            ))?;
        }
        for spec in figures::mixing_matrix::specs(&table, save_dir) {
            self.push(spec)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn sensitivity(
        &self,
        pairs: &[String],
        ari: &[f64],
        ami: &[f64],
        runs: &[String],
        matrix: &[f64],
        reference: &str,
        niches: &[u32],
        stability_runs: &[String],
        stability: &[f64],
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        self.push(figures::sensitivity::agreement(pairs, ari, ami, save_dir))?;
        self.push(figures::sensitivity::matrix(runs, matrix, save_dir))?;
        self.push(figures::sensitivity::stability(
            reference,
            niches,
            stability_runs,
            stability,
            save_dir,
        ))
    }

    fn niche_composition(
        &self,
        composition: &NicheComposition,
        niches: &[u32],
        normalize: Normalize,
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        self.push(figures::composition::spec(composition, normalize, save_dir))?;
        self.push(figures::histogram::spec(niches, save_dir))
    }

    fn embedding(
        &self,
        embedding: &[f64],
        n_components: usize,
        labels: &[u32],
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        match figures::embedding::spec(embedding, n_components, labels, "", save_dir) {
            Some(spec) => self.push(spec),
            // A projection with fewer than two dimensions cannot be scattered
            // in a plane. Nothing is drawn rather than something misleading.
            None => Ok(()),
        }
    }
}

/// The batches of figures set aside by [`Figures::defer`], oldest first.
pub fn deferred_batches(working_dir: &Path) -> Vec<PathBuf> {
    let directory = working_dir.join(mosna_pipeline::figures::DEFERRED_FIGURE_DIRECTORY);
    let mut batches: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    // Zero-padded timestamps: the name order is the order the runs ended in.
    batches.sort();
    batches
}

/// Draw every figure set aside under `working_dir`, without computing anything.
///
/// Each batch is removed once drawn, so pressing the button twice draws
/// nothing the second time, and a batch that fails is kept, as a failed
/// render's queue always is: it is the exact input that failed. The other
/// batches are drawn regardless — one bad figure must not cost the rest.
///
/// Unlike an analysis, a missing renderer *is* an error here: drawing is the
/// only thing that was asked for.
///
/// Returns how many batches were drawn.
pub fn render_deferred<R: Run>(
    working_dir: &Path,
    renderer: &Renderer<R>,
) -> anyhow::Result<usize> {
    let batches = deferred_batches(working_dir);
    if batches.is_empty() {
        println!(
            "[QT_INFO] No figures are waiting to be drawn: every analysis run in this \
             directory has already drawn its own."
        );
        return Ok(0);
    }

    let mut drawn = 0;
    let mut failures = Vec::new();
    for (index, batch) in batches.iter().enumerate() {
        println!(
            "[QT_INFO] Drawing figures set aside, batch {} of {}",
            index + 1,
            batches.len()
        );
        match renderer.render(batch) {
            Ok(()) => {
                drawn += 1;
                if let Err(error) = std::fs::remove_dir_all(batch) {
                    log_discard_failure(&error);
                }
            }
            Err(failure) => {
                if let Err(reason) = renderer.check() {
                    return Err(anyhow::anyhow!(
                        "{reason}\nThe figures are kept and can be drawn once the renderer \
                         is installed: install MOSNA again with INSTALLATION.exe (figures \
                         ticked) or install.sh (without --no-figures)."
                    ));
                }
                failures.push(format!("{}: {failure}", batch.display()));
            }
        }
    }

    // And the directory itself, once nothing is left in it.
    let _ =
        std::fs::remove_dir(working_dir.join(mosna_pipeline::figures::DEFERRED_FIGURE_DIRECTORY));

    if failures.is_empty() {
        println!("[QT_INFO] Every figure set aside is drawn");
        Ok(drawn)
    } else {
        Err(anyhow::anyhow!(
            "{} of {} batches of figures could not be drawn; their specifications are kept:\n{}",
            failures.len(),
            batches.len(),
            failures.join("\n")
        ))
    }
}

/// A queue that could not be removed costs disk space and nothing else: the
/// figures it described are already on disk. It is reported, not raised.
fn log_discard_failure(error: &std::io::Error) {
    eprintln!("[QT_INFO] the figure queue could not be removed: {error}");
}

/// A queueing failure must not read like a computation failure, so it says
/// what it was doing.
fn to_pipeline_error(error: anyhow::Error) -> mosna_pipeline::PipelineError {
    mosna_pipeline::PipelineError::invalid(format!("cannot describe a figure: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::Outcome;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// A renderer that answers `check` and `render` separately, because the two
    /// now mean different things: whether the renderer is installed at all, and
    /// whether it managed to draw.
    struct Recording {
        installed: bool,
        draws: bool,
        calls: Mutex<Vec<(PathBuf, Vec<String>)>>,
    }

    impl Recording {
        fn new(installed: bool, draws: bool) -> Self {
            Self {
                installed,
                draws,
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl Run for Recording {
        fn run(&self, program: &Path, arguments: &[String]) -> std::io::Result<Outcome> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_path_buf(), arguments.to_vec()));

            let checking = arguments.iter().any(|a| a == "check");
            Ok(if checking {
                Outcome {
                    success: self.installed,
                    message: "No module named mosna_xy".to_string(),
                }
            } else {
                Outcome {
                    success: self.draws,
                    message: "cannot draw 00000-embedding: no such colour".to_string(),
                }
            })
        }
    }

    /// An installed renderer that draws what it is given.
    fn figures(working_dir: &Path, success: bool) -> Figures<Recording> {
        Figures::with_renderer(
            working_dir,
            Renderer::with("python3", Recording::new(true, success)),
        )
    }

    /// No renderer on the machine at all — an install made without it.
    fn figures_without_renderer(working_dir: &Path) -> Figures<Recording> {
        Figures::with_renderer(
            working_dir,
            Renderer::with("python3", Recording::new(false, false)),
        )
    }

    fn net_stat() -> (Vec<String>, Vec<(String, Vec<f64>)>) {
        (
            vec![
                "# total".into(),
                "% A".into(),
                "% B".into(),
                "assort Z".into(),
                "A - A Z".into(),
                "A - B Z".into(),
                "B - B Z".into(),
            ],
            vec![
                (
                    "patient-1_sample-1".to_string(),
                    vec![10.0, 0.6, 0.4, 3.0, 1.0, -2.0, 0.5],
                ),
                (
                    "patient-2_sample-1".to_string(),
                    vec![20.0, 0.5, 0.5, 1.0, 0.2, -1.0, 0.1],
                ),
            ],
        )
    }

    #[test]
    fn a_network_is_one_figure() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);

        figures
            .network(
                &SampleId::patient_only("1"),
                "patient",
                None,
                &[[0.0, 0.0]],
                &[],
                &["A".to_string()],
                dir.path(),
            )
            .unwrap();

        assert_eq!(figures.queued(), 1);
    }

    /// Six figures about the cohort, and two per sample: that is the inventory
    /// step two has always produced, and the interface's gallery is built
    /// around finding exactly those files.
    #[test]
    fn assortativity_queues_the_whole_inventory() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);
        let (columns, rows) = net_stat();

        figures.assortativity(&columns, &rows, dir.path()).unwrap();

        assert_eq!(figures.queued(), 5 + 2 * rows.len());
    }

    #[test]
    fn a_niche_composition_brings_its_histogram_along() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);
        let composition = NicheComposition {
            phenotypes: vec!["A".to_string()],
            niches: vec![0],
            counts: vec![1.0],
        };

        figures
            .niche_composition(&composition, &[0, 0, 1], Normalize::Total, dir.path())
            .unwrap();

        assert_eq!(figures.queued(), 2);
    }

    #[test]
    fn a_projection_that_is_not_a_plane_queues_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);

        figures
            .embedding(&[1.0, 2.0], 1, &[0, 1], dir.path())
            .unwrap();
        assert_eq!(figures.queued(), 0);

        figures.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();
        assert_eq!(figures.queued(), 1);
    }

    #[test]
    fn drawing_hands_the_queue_over_and_then_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);
        figures.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();

        figures.render().unwrap();

        let calls = figures.renderer.runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0]
            .1
            .iter()
            .any(|argument| argument.contains(".mosna-figures")));
        assert!(
            !dir.path().join(".mosna-figures").exists(),
            "the queue outlived the figures it described"
        );
    }

    /// An analysis that drew nothing must not start an interpreter to be told
    /// there is nothing to draw — `clear-temporary` and a run with no samples
    /// both end up here.
    #[test]
    fn nothing_queued_starts_no_interpreter() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);

        figures.render().unwrap();

        assert!(figures.renderer.runner.calls.lock().unwrap().is_empty());
    }

    /// A failed render keeps the queue: it is the exact input that produced
    /// the failure, and the alternative is re-running the analysis to get it
    /// back.
    #[test]
    fn a_failed_render_leaves_the_queue_where_it_can_be_looked_at() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), false);
        figures.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();

        let error = figures.render().unwrap_err().to_string();

        assert!(error.contains("00000-embedding"), "{error}");
        assert!(dir.path().join(".mosna-figures").is_dir());
    }

    /// An install without the renderer is a supported, smaller install: the
    /// analysis is finished and its tables are written, so it must not be
    /// reported as a failure.
    #[test]
    fn an_absent_renderer_is_not_a_failed_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures_without_renderer(dir.path());
        figures.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();

        figures
            .render()
            .expect("a missing renderer is not an error");

        // The queue survives, so installing the renderer later lets the next
        // run draw what this one could not.
        assert!(dir.path().join(".mosna-figures").is_dir());
    }

    /// A renderer that is installed and fails is still an error: that is
    /// something broken, not something the user chose not to install.
    #[test]
    fn an_installed_renderer_that_fails_is_still_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), false);
        figures.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();

        let error = figures.render().unwrap_err().to_string();
        assert!(error.contains("00000-embedding"), "{error}");
    }

    // -----------------------------------------------------------------------
    // Figures set aside, drawn later
    // -----------------------------------------------------------------------

    /// A run started without plotting draws nothing, and leaves its figures
    /// where `clear-temporary` will not take them.
    #[test]
    fn deferring_draws_nothing_and_keeps_the_figures() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);
        figures.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();

        assert_eq!(figures.defer().unwrap(), 1);

        assert!(figures.renderer.runner.calls.lock().unwrap().is_empty());
        assert!(!dir.path().join(".mosna-figures").exists());
        let batches = deferred_batches(dir.path());
        assert_eq!(batches.len(), 1);
        assert!(batches[0].starts_with(dir.path().join(".mosna-figures-deferred")));
        assert!(batches[0].join("00000-embedding/figure.json").is_file());
    }

    #[test]
    fn deferring_nothing_leaves_no_trace() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(figures(dir.path(), true).defer().unwrap(), 0);
        assert!(!dir.path().join(".mosna-figures-deferred").exists());
    }

    /// Two runs set aside are drawn in the order they ended, so a file both
    /// describe ends up as the later one drew it; and once drawn, they are gone.
    #[test]
    fn the_figures_set_aside_are_drawn_oldest_first_then_removed() {
        let dir = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            let run = figures(dir.path(), true);
            run.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();
            run.defer().unwrap();
        }
        let batches = deferred_batches(dir.path());
        assert_eq!(batches.len(), 2);

        let renderer = Renderer::with("python3", Recording::new(true, true));
        assert_eq!(render_deferred(dir.path(), &renderer).unwrap(), 2);

        let calls = renderer.runner.calls.lock().unwrap();
        let drawn: Vec<&String> = calls.iter().map(|(_, arguments)| &arguments[3]).collect();
        assert_eq!(
            drawn,
            batches
                .iter()
                .map(|b| b.display().to_string())
                .collect::<Vec<_>>()
                .iter()
                .collect::<Vec<_>>()
        );
        assert!(!dir.path().join(".mosna-figures-deferred").exists());
    }

    #[test]
    fn nothing_set_aside_is_nothing_to_draw() {
        let dir = tempfile::tempdir().unwrap();
        let renderer = Renderer::with("python3", Recording::new(true, true));
        assert_eq!(render_deferred(dir.path(), &renderer).unwrap(), 0);
        assert!(renderer.runner.calls.lock().unwrap().is_empty());
    }

    /// A batch that will not draw is kept, as a failed queue always is.
    #[test]
    fn a_batch_that_fails_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let run = figures(dir.path(), true);
        run.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();
        run.defer().unwrap();

        let renderer = Renderer::with("python3", Recording::new(true, false));
        assert!(render_deferred(dir.path(), &renderer).is_err());
        assert_eq!(deferred_batches(dir.path()).len(), 1);
    }

    /// Here, drawing is all that was asked for: no renderer is an error, and
    /// the figures stay for when there is one.
    #[test]
    fn drawing_without_a_renderer_is_an_error_that_keeps_the_figures() {
        let dir = tempfile::tempdir().unwrap();
        let run = figures(dir.path(), true);
        run.embedding(&[1.0, 2.0], 2, &[0], dir.path()).unwrap();
        run.defer().unwrap();

        let renderer = Renderer::with("python3", Recording::new(false, false));
        let error = render_deferred(dir.path(), &renderer)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not usable"), "{error}");
        assert_eq!(deferred_batches(dir.path()).len(), 1);
    }

    /// The trait is used as `&dyn FigureSink` by every pipeline, so it has to
    /// stay object-safe with this implementation behind it.
    #[test]
    fn the_sink_is_still_a_trait_object() {
        let dir = tempfile::tempdir().unwrap();
        let figures = figures(dir.path(), true);
        let sink: &dyn FigureSink = &figures;

        sink.assortativity(&[], &[], dir.path()).unwrap();
    }
}
