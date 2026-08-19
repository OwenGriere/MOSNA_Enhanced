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

use std::path::Path;

use mosna_core::niches::{NicheComposition, Normalize};
use mosna_io::SampleId;
use mosna_pipeline::FigureSink;

use crate::renderer::{Renderer, Run, Subprocess};
use crate::spec::{Queue, Spec};
use crate::table::Table;

/// Queues every figure an analysis produces, then has them drawn.
pub struct Figures<R: Run = Subprocess> {
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
    pub fn render(&self) -> anyhow::Result<()> {
        if self.queue.is_empty() {
            return Ok(());
        }

        self.renderer.render(self.queue.directory())?;

        // Only once the figures exist. A failed render keeps its queue: that
        // is the exact input that produced the failure, and the alternative is
        // running the analysis again to get it back.
        if let Err(error) = self.queue.discard() {
            log_discard_failure(&error);
        }
        Ok(())
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

    struct Recording {
        outcome: Outcome,
        calls: Mutex<Vec<(PathBuf, Vec<String>)>>,
    }

    impl Recording {
        fn new(success: bool) -> Self {
            Self {
                outcome: Outcome {
                    success,
                    message: "cannot draw 00000-embedding: no such colour".to_string(),
                },
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
            Ok(self.outcome.clone())
        }
    }

    fn figures(working_dir: &Path, success: bool) -> Figures<Recording> {
        Figures::with_renderer(
            working_dir,
            Renderer::with("python3", Recording::new(success)),
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
