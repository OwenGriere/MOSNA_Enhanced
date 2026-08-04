//! Synthetic tissue, generated to order.
//!
//! The repository ships one real cohort of 39 290 cells. That is enough to
//! check correctness and far too small — and far too fixed — to say anything
//! about how the port scales. This module produces tissue of any size, with the
//! structure a spatial analysis is supposed to find.
//!
//! # The model
//!
//! Each sample holds `n_niches` niche centres scattered over the field. Every
//! niche has its own phenotype mixture, drawn once per sample. A cell picks a
//! niche, lands near that niche's centre, and takes a phenotype from the
//! niche's mixture.
//!
//! That is deliberately the generative model the niche analysis assumes, which
//! is the point: the planted niches are recoverable, so level 3 can score what
//! the pipeline recovers against what was planted. Uniform noise would leave
//! every partition as good as every other.

use std::path::Path;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use mosna_io::write::write_parquet::write_parquet;
use mosna_io::Table;

/// How much tissue to make.
#[derive(Debug, Clone)]
pub struct CohortSpec {
    pub n_samples: usize,
    pub cells_per_sample: usize,
    pub n_phenotypes: usize,
    pub n_niches: usize,
    /// Every random draw descends from this, so a run is reproducible.
    pub seed: u64,
}

impl Default for CohortSpec {
    /// A cohort close to the shape of the repository's real one: four samples,
    /// ten thousand cells each, a realistic phenotype vocabulary.
    fn default() -> Self {
        Self {
            n_samples: 4,
            cells_per_sample: 10_000,
            n_phenotypes: 12,
            n_niches: 5,
            seed: 20_260_804,
        }
    }
}

/// One sample's worth of cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Tissue {
    pub coords: Vec<[f64; 2]>,
    pub phenotypes: Vec<String>,
    /// The niche each cell was drawn from — the ground truth level 3 scores
    /// against.
    pub niches: Vec<u32>,
}

/// Side of the square field, in arbitrary units.
const FIELD: f64 = 1000.0;

impl Tissue {
    /// Generate sample `index` of the cohort described by `spec`.
    ///
    /// The seed mixes in the sample index, so the samples of one cohort differ
    /// from each other while the cohort as a whole stays reproducible.
    pub fn generate(spec: &CohortSpec, index: usize) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(spec.seed ^ ((index as u64 + 1) << 32));

        let centres: Vec<[f64; 2]> = (0..spec.n_niches)
            .map(|_| [rng.gen_range(0.0..FIELD), rng.gen_range(0.0..FIELD)])
            .collect();

        // Each niche gets a lopsided phenotype mixture: two or three dominant
        // phenotypes and a tail. A uniform mixture would make every niche look
        // the same to the composition heatmap.
        let mixtures: Vec<Vec<f64>> = (0..spec.n_niches)
            .map(|_| {
                let weights: Vec<f64> = (0..spec.n_phenotypes)
                    .map(|_| {
                        let u: f64 = rng.gen_range(0.0..1.0);
                        // Cubing spreads the weights out: a few large, many small.
                        u * u * u + 0.01
                    })
                    .collect();
                let total: f64 = weights.iter().sum();
                weights.iter().map(|w| w / total).collect()
            })
            .collect();

        // Niches differ in size, as real ones do.
        let sizes: Vec<f64> = (0..spec.n_niches)
            .map(|_| rng.gen_range(0.5..1.5))
            .collect();
        let total_size: f64 = sizes.iter().sum();

        // A niche's cells sit within roughly this radius of its centre. Scaled
        // to the field and the niche count so niches touch without merging.
        let spread = FIELD / (spec.n_niches as f64).sqrt() / 4.0;

        let mut coords = Vec::with_capacity(spec.cells_per_sample);
        let mut phenotypes = Vec::with_capacity(spec.cells_per_sample);
        let mut niches = Vec::with_capacity(spec.cells_per_sample);

        for _ in 0..spec.cells_per_sample {
            let niche = pick(&sizes, total_size, &mut rng);

            let (dx, dy) = gaussian_pair(&mut rng);
            coords.push([
                (centres[niche][0] + dx * spread).clamp(0.0, FIELD),
                (centres[niche][1] + dy * spread).clamp(0.0, FIELD),
            ]);

            let phenotype = pick(&mixtures[niche], 1.0, &mut rng);
            phenotypes.push(format!("pheno_{phenotype}"));
            niches.push(niche as u32);
        }

        Self {
            coords,
            phenotypes,
            niches,
        }
    }

    /// The cells as the table the pipelines read.
    pub fn to_table(&self) -> anyhow::Result<Table> {
        let table = Table::from_columns(vec![
            (
                "X_position".into(),
                Table::f64_array(self.coords.iter().map(|p| p[0])),
            ),
            (
                "Y_position".into(),
                Table::f64_array(self.coords.iter().map(|p| p[1])),
            ),
            (
                "Cluster".into(),
                Table::string_array(self.phenotypes.iter().map(String::as_str)),
            ),
            (
                "true_niche".into(),
                Table::f64_array(self.niches.iter().map(|&n| n as f64)),
            ),
        ])?;
        Ok(table)
    }
}

/// Draw an index with probability proportional to `weights`.
fn pick(weights: &[f64], total: f64, rng: &mut ChaCha8Rng) -> usize {
    let mut threshold = rng.gen_range(0.0..total);
    for (index, weight) in weights.iter().enumerate() {
        threshold -= weight;
        if threshold <= 0.0 {
            return index;
        }
    }
    weights.len() - 1
}

/// Two independent standard normal deviates, by Box-Muller.
///
/// Written out rather than taken from `rand_distr` so the stream depends only
/// on `rand`'s uniform generator — one dependency fewer between the seed and
/// the tissue, and one fewer reason for a golden fingerprint to move when a
/// crate is upgraded.
fn gaussian_pair(rng: &mut ChaCha8Rng) -> (f64, f64) {
    let u1: f64 = rng.gen_range(f64::MIN_POSITIVE..1.0);
    let u2: f64 = rng.gen_range(0.0..1.0);
    let radius = (-2.0 * u1.ln()).sqrt();
    let angle = std::f64::consts::TAU * u2;
    (radius * angle.cos(), radius * angle.sin())
}

/// Write a whole cohort as `nodes_patient-N_sample-1.parquet`, the layout step
/// one reads.
pub fn write_cohort(spec: &CohortSpec, dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    for index in 0..spec.n_samples {
        let tissue = Tissue::generate(spec, index);
        let name = format!("nodes_patient-{}_sample-1.parquet", index + 1);
        write_parquet(&tissue.to_table()?, dir.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weighted_draw_respects_its_weights() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let weights = [0.9, 0.1];
        let mut first = 0;
        for _ in 0..1000 {
            if pick(&weights, 1.0, &mut rng) == 0 {
                first += 1;
            }
        }
        assert!((850..=950).contains(&first), "got {first} out of 1000");
    }

    #[test]
    fn the_gaussian_pair_has_the_expected_spread() {
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut sum = 0.0;
        let mut sum_squares = 0.0;
        let n = 4000;
        for _ in 0..n / 2 {
            let (a, b) = gaussian_pair(&mut rng);
            sum += a + b;
            sum_squares += a * a + b * b;
        }
        let mean = sum / n as f64;
        let variance = sum_squares / n as f64 - mean * mean;
        assert!(mean.abs() < 0.1, "mean {mean}");
        assert!((variance - 1.0).abs() < 0.1, "variance {variance}");
    }
}
