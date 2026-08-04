//! Port of `assortativity.py::{core_rand_mixmat, randomized_mixmat}`.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use crate::assortativity::attribute_ac::attribute_ac;
use crate::assortativity::mixing_matrix::{mixing_matrix, MixMat};
use crate::Pair;

/// Per-element mean and population standard deviation of the null mixing
/// matrices, plus the assortativity coefficient of each randomisation.
pub struct NullDistribution {
    pub mixmat_mean: MixMat,
    pub mixmat_std: MixMat,
    pub assort: Vec<f64>,
}

/// Base seed, chosen so a run is reproducible.
///
/// The Python calls `np.random.seed(0)` before shuffling, for the same reason.
const SEED: u64 = 0;

/// Build the null distribution by shuffling node attributes `n_shuffle` times.
///
/// Each repetition permutes which node carries which phenotype while leaving
/// the edges untouched, so the null preserves the network topology and the
/// phenotype abundances but destroys any spatial organisation.
///
/// # Random numbers differ from Python
///
/// The Python draws its permutations from numpy's global generator seeded with
/// `np.random.seed(0)`. Reproducing numpy's Mersenne Twister stream and
/// scikit-learn's exact consumption of it is not practical, so this uses its
/// own seeded generator. The permutations are therefore different, and z-scores
/// differ by the Monte Carlo error of the test — the same order of magnitude as
/// between two Python runs with different seeds. Raising `Number of shuffle`
/// shrinks it. Results here are reproducible run to run, and independent of how
/// many threads are used, because each repetition derives its own seed from its
/// index.
pub fn randomized_mixmat(
    assignments: &[Option<u32>],
    pairs: &[Pair],
    n_attributes: usize,
    n_shuffle: usize,
) -> NullDistribution {
    if n_shuffle == 0 {
        return NullDistribution {
            mixmat_mean: MixMat::zeros(n_attributes),
            mixmat_std: MixMat::zeros(n_attributes),
            assort: Vec::new(),
        };
    }

    // Welford accumulators, merged across threads: storing every one of the
    // `n_shuffle` matrices would cost n_shuffle * n^2 * 8 bytes, which for a
    // few hundred phenotypes and 500 shuffles runs into hundreds of megabytes.
    let accumulator = (0..n_shuffle)
        .into_par_iter()
        .map(|repetition| {
            let mut rng = ChaCha8Rng::seed_from_u64(SEED.wrapping_add(repetition as u64));
            let mut shuffled = assignments.to_vec();
            shuffled.shuffle(&mut rng);

            let mixmat = mixing_matrix(&shuffled, pairs, n_attributes, true, true);
            let assort = attribute_ac(&mixmat);
            Accumulator::single(mixmat, assort, repetition)
        })
        .reduce(|| Accumulator::empty(n_attributes), Accumulator::merge);

    accumulator.finish()
}

/// Streaming mean and variance of the null matrices.
struct Accumulator {
    count: usize,
    mean: Vec<f64>,
    /// Sum of squared deviations from the running mean.
    m2: Vec<f64>,
    /// `(repetition, coefficient)` pairs, sorted on finish so the assortativity
    /// vector does not depend on thread scheduling.
    assort: Vec<(usize, f64)>,
    n: usize,
}

impl Accumulator {
    fn empty(n: usize) -> Self {
        Self {
            count: 0,
            mean: vec![0.0; n * n],
            m2: vec![0.0; n * n],
            assort: Vec::new(),
            n,
        }
    }

    fn single(mixmat: MixMat, assort: f64, repetition: usize) -> Self {
        let n = mixmat.n;
        Self {
            count: 1,
            mean: mixmat.values,
            m2: vec![0.0; n * n],
            assort: vec![(repetition, assort)],
            n,
        }
    }

    /// Chan's parallel variance merge.
    fn merge(mut self, other: Self) -> Self {
        if self.count == 0 {
            return other;
        }
        if other.count == 0 {
            return self;
        }
        let n_a = self.count as f64;
        let n_b = other.count as f64;
        let total = n_a + n_b;

        for i in 0..self.mean.len() {
            let delta = other.mean[i] - self.mean[i];
            self.mean[i] += delta * n_b / total;
            self.m2[i] += other.m2[i] + delta * delta * n_a * n_b / total;
        }
        self.count += other.count;
        self.assort.extend(other.assort);
        self
    }

    fn finish(mut self) -> NullDistribution {
        let count = self.count.max(1) as f64;
        self.assort
            .sort_unstable_by_key(|(repetition, _)| *repetition);

        NullDistribution {
            mixmat_mean: MixMat {
                n: self.n,
                values: self.mean,
            },
            mixmat_std: MixMat {
                n: self.n,
                // Population standard deviation, matching `ndarray.std()`.
                values: self.m2.iter().map(|m2| (m2 / count).sqrt()).collect(),
            },
            assort: self.assort.into_iter().map(|(_, value)| value).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ring of 12 nodes, alternating between two phenotypes.
    fn ring() -> (Vec<Option<u32>>, Vec<Pair>) {
        let n = 12u32;
        let assignments: Vec<Option<u32>> = (0..n).map(|i| Some(i % 2)).collect();
        let pairs: Vec<Pair> = (0..n).map(|i| (i, (i + 1) % n)).collect();
        (assignments, pairs)
    }

    #[test]
    fn produces_one_coefficient_per_repetition() {
        let (assignments, pairs) = ring();
        let null = randomized_mixmat(&assignments, &pairs, 2, 50);
        assert_eq!(null.assort.len(), 50);
        assert_eq!(null.mixmat_mean.n, 2);
    }

    #[test]
    fn the_null_is_reproducible() {
        let (assignments, pairs) = ring();
        let a = randomized_mixmat(&assignments, &pairs, 2, 30);
        let b = randomized_mixmat(&assignments, &pairs, 2, 30);
        assert_eq!(a.assort, b.assort);
        assert_eq!(a.mixmat_mean.values, b.mixmat_mean.values);
        assert_eq!(a.mixmat_std.values, b.mixmat_std.values);
    }

    #[test]
    fn the_null_preserves_the_total_edge_mass() {
        // Each randomised matrix is normalised, so its mean must be too.
        let (assignments, pairs) = ring();
        let null = randomized_mixmat(&assignments, &pairs, 2, 40);
        assert!((null.mixmat_mean.sum() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn shuffling_destroys_a_planted_structure() {
        // A perfectly disassortative ring: neighbours always differ. The null,
        // which breaks the spatial arrangement, must score closer to zero.
        let (assignments, pairs) = ring();
        let observed = attribute_ac(&mixing_matrix(&assignments, &pairs, 2, true, true));
        let null = randomized_mixmat(&assignments, &pairs, 2, 200);
        let null_mean = null.assort.iter().sum::<f64>() / null.assort.len() as f64;

        assert!(
            observed < -0.9,
            "the ring is disassortative, got {observed}"
        );
        assert!(
            null_mean > observed + 0.5,
            "the null must be far less structured: {null_mean} vs {observed}"
        );
    }

    #[test]
    fn the_standard_deviation_matches_a_direct_computation() {
        let (assignments, pairs) = ring();
        let n_shuffle = 25;
        let null = randomized_mixmat(&assignments, &pairs, 2, n_shuffle);

        // Recompute element (0, 0) the naive way.
        let mut samples = Vec::new();
        for repetition in 0..n_shuffle {
            let mut rng = ChaCha8Rng::seed_from_u64(SEED.wrapping_add(repetition as u64));
            let mut shuffled = assignments.clone();
            shuffled.shuffle(&mut rng);
            samples.push(mixing_matrix(&shuffled, &pairs, 2, true, true).get(0, 0));
        }
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let std =
            (samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();

        assert!((null.mixmat_mean.get(0, 0) - mean).abs() < 1e-12);
        assert!((null.mixmat_std.get(0, 0) - std).abs() < 1e-12);
    }

    #[test]
    fn zero_shuffles_yields_an_empty_null() {
        let (assignments, pairs) = ring();
        let null = randomized_mixmat(&assignments, &pairs, 2, 0);
        assert!(null.assort.is_empty());
    }
}
