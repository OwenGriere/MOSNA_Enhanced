//! The seam between the analyses and the plotting crate.
//!
//! Figures are the last piece of the port. Rather than interleave plotting
//! calls with the computation and leave the pipelines unrunnable until the
//! plotting crate exists, every figure the Python produces is a method on this
//! trait with a do-nothing default. `mosna-viz` will implement it; until then
//! [`NoFigures`] lets the whole data pipeline run and be tested.
//!
//! The method list is the inventory of what `mosna-viz` owes, taken from
//! `package/core/*/`.

use std::path::Path;

use mosna_core::niches::{NicheComposition, Normalize};
use mosna_io::SampleId;

/// Receives everything the analyses would plot.
pub trait FigureSink: Sync {
    /// The spatial network of one sample, coloured by phenotype.
    ///
    /// From `package/core/tysserand/draw_per_sample.py`; written as
    /// `Tysserand_Network/net_{patient}-{sample}.png`.
    ///
    /// The column names come along because the figure's title spells them out,
    /// as the Python's does.
    #[allow(clippy::too_many_arguments)]
    fn network(
        &self,
        _sample: &SampleId,
        _patient_column: &str,
        _sample_column: Option<&str>,
        _coords: &[[f64; 2]],
        _pairs: &[(u32, u32)],
        _labels: &[String],
        _save_dir: &Path,
    ) -> crate::Result<()> {
        Ok(())
    }

    /// The six assortativity figures, from `package/core/assortativity/`.
    ///
    /// `columns` and `rows` are the `net_stat` table: column names, and one row
    /// of values per sample keyed by its id.
    fn assortativity(
        &self,
        _columns: &[String],
        _rows: &[(String, Vec<f64>)],
        _save_dir: &Path,
    ) -> crate::Result<()> {
        Ok(())
    }

    /// Niche composition heatmap and niche histogram, from
    /// `package/core/NAS/mosna_figures.py`.
    fn niche_composition(
        &self,
        _composition: &NicheComposition,
        _niches: &[u32],
        _normalize: Normalize,
        _save_dir: &Path,
    ) -> crate::Result<()> {
        Ok(())
    }

    /// The clustered 2-D projection, from `package/core/NAS/plot_embedding.py`.
    fn embedding(
        &self,
        _embedding: &[f64],
        _n_components: usize,
        _labels: &[u32],
        _save_dir: &Path,
    ) -> crate::Result<()> {
        Ok(())
    }
}

/// Produces no figures.
///
/// Used by the tests, and by the CLI until `mosna-viz` lands.
pub struct NoFigures;

impl FigureSink for NoFigures {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_sink_accepts_every_call() {
        let sink = NoFigures;
        let dir = std::path::Path::new("/tmp");
        sink.network(
            &SampleId::patient_only("1"),
            "patient",
            None,
            &[[0.0, 0.0]],
            &[(0, 0)],
            &["A".to_string()],
            dir,
        )
        .unwrap();
        sink.assortativity(&[], &[], dir).unwrap();
        sink.embedding(&[], 2, &[], dir).unwrap();
    }

    /// The pipelines take `&dyn FigureSink`, so the trait has to be
    /// object-safe for `mosna-viz` to be swapped in later.
    #[test]
    fn the_trait_is_object_safe() {
        let sink: &dyn FigureSink = &NoFigures;
        sink.assortativity(&[], &[], std::path::Path::new("/tmp"))
            .unwrap();
    }
}
