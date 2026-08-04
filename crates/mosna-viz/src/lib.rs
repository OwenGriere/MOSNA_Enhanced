//! Figures produced by the MOSNA analyses.
//!
//! Implements [`mosna_pipeline::FigureSink`], the seam the pipelines call into.
//! Until this crate existed the analyses ran and produced no images; wiring it
//! in is a one-line change at the call site.
//!
//! # Fidelity
//!
//! The file names, the directories, the colour maps and the normalisations are
//! reproduced exactly — those are what the interface scans for and what makes
//! a z-score readable. The layout is close but not pixel-identical to
//! matplotlib: reproducing its exact axis placement would pin the arrangement
//! rather than the meaning, and matplotlib is not available to compare against.

pub mod assortativity;
pub mod canvas;
pub mod embedding;
pub mod network;
pub mod niches;
pub mod norm;
pub mod theme;

use std::path::Path;

/// The colour maps, which live in `mosna-core` so that the interface can draw
/// a network in the same colours as the figures without taking a dependency on
/// the plotting crate. Re-exported here because this is where they are used
/// most, and where they used to be.
pub use mosna_core::colormap;

use mosna_core::niches::{NicheComposition, Normalize};
use mosna_io::SampleId;
use mosna_pipeline::FigureSink;

pub use theme::Theme;

/// Draws every figure the analyses produce.
#[derive(Debug, Clone, Default)]
pub struct Figures {
    theme: Theme,
}

impl Figures {
    /// Figures at the default resolution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Figures at a chosen resolution.
    ///
    /// `Theme { dpi: 300.0 }` matches the Python's pixel dimensions exactly.
    pub fn with_theme(theme: Theme) -> Self {
        Self { theme }
    }

    /// A low resolution, so a test suite is not spent encoding megapixels.
    pub fn for_tests() -> Self {
        Self::with_theme(Theme { dpi: 12.0 })
    }
}

impl FigureSink for Figures {
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
        network::draw(
            &self.theme,
            sample,
            patient_column,
            sample_column,
            coords,
            pairs,
            labels,
            save_dir,
        )
        .map_err(to_pipeline_error)
    }

    fn assortativity(
        &self,
        columns: &[String],
        rows: &[(String, Vec<f64>)],
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        let table = assortativity::Table::new(columns, rows);

        assortativity::abundance::draw(&self.theme, &table, save_dir).map_err(to_pipeline_error)?;
        for include_self_pairs in [true, false] {
            assortativity::heatmap::draw(&self.theme, &table, include_self_pairs, save_dir)
                .map_err(to_pipeline_error)?;
            assortativity::mean_std::draw(&self.theme, &table, include_self_pairs, save_dir)
                .map_err(to_pipeline_error)?;
        }
        assortativity::mixing_matrix::draw_all(&self.theme, &table, save_dir)
            .map_err(to_pipeline_error)
    }

    fn niche_composition(
        &self,
        composition: &NicheComposition,
        niches: &[u32],
        normalize: Normalize,
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        niches::composition::draw(&self.theme, composition, normalize, save_dir)
            .map_err(to_pipeline_error)?;
        niches::histogram::draw(&self.theme, niches, save_dir).map_err(to_pipeline_error)
    }

    fn embedding(
        &self,
        embedding: &[f64],
        n_components: usize,
        labels: &[u32],
        save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        embedding::draw(&self.theme, embedding, n_components, labels, "", save_dir)
            .map_err(to_pipeline_error)
    }
}

/// A drawing failure must not read like a computation failure, so it says what
/// it was doing.
fn to_pipeline_error(error: anyhow::Error) -> mosna_pipeline::PipelineError {
    mosna_pipeline::PipelineError::invalid(format!("cannot draw a figure: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_resolution_is_small_enough_to_be_cheap() {
        let (width, height) = Figures::for_tests().theme.canvas(30.0, 30.0);
        assert!(width * height < 200_000, "{width}x{height} is too large");
    }

    #[test]
    fn the_default_resolution_is_the_documented_one() {
        assert_eq!(Figures::new().theme.dpi, 100.0);
    }
}
