//! Port of `mosna/niches.py::make_niches_composition`.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{CoreError, Result};
use crate::stats::clr::transform_clr;

/// How the composition counts are rescaled before plotting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Normalize {
    /// Raw counts. Not reachable from the configuration, but useful to test
    /// the counting separately from the rescaling.
    None,
    /// Divide by the total number of cells.
    Total,
    /// Each niche sums to one: what is this niche made of?
    Niche,
    /// Each phenotype sums to one: where does this cell type live?
    Obs,
    /// Centred log-ratio.
    Clr,
    /// Per phenotype, then per niche.
    NicheAndObs,
}

impl Normalize {
    /// Parse the spelling used in `configuration.yaml`.
    pub fn parse(name: &str) -> Self {
        match name {
            "niche" => Normalize::Niche,
            "obs" => Normalize::Obs,
            "clr" => Normalize::Clr,
            "niche&obs" => Normalize::NicheAndObs,
            _ => Normalize::Total,
        }
    }

    /// The spelling used in output file names.
    pub fn as_str(self) -> &'static str {
        match self {
            Normalize::None => "none",
            Normalize::Total => "total",
            Normalize::Niche => "niche",
            Normalize::Obs => "obs",
            Normalize::Clr => "clr",
            Normalize::NicheAndObs => "niche&obs",
        }
    }
}

/// Cell-type composition of each niche.
#[derive(Debug, Clone, PartialEq)]
pub struct NicheComposition {
    /// Row labels, sorted.
    pub phenotypes: Vec<String>,
    /// Column labels, sorted.
    pub niches: Vec<u32>,
    /// Row-major `phenotypes.len() * niches.len()`.
    pub counts: Vec<f64>,
}

impl NicheComposition {
    pub fn get(&self, phenotype: usize, niche: usize) -> f64 {
        self.counts[phenotype * self.niches.len() + niche]
    }
}

/// Count cells by phenotype and niche, then rescale.
///
/// Port of
///
/// ```python
/// df = pd.DataFrame({var_label: var, 'niches': niches})
/// df['counts'] = np.arange(df.shape[0])
/// counts = df.groupby([var_label, 'niches']).count()
/// counts = counts.reset_index().pivot(index=var_label, columns='niches',
///                                     values='counts').fillna(0)
/// ```
///
/// `groupby` and `pivot` both sort, so rows come out ordered by phenotype name
/// and columns by niche id; that ordering is what the figures label their axes
/// with, so it is reproduced here.
pub fn make_niches_composition(
    var: &[String],
    niches: &[u32],
    normalize: Normalize,
) -> Result<NicheComposition> {
    if var.len() != niches.len() {
        return Err(CoreError::shape(format!(
            "{} cell types but {} niche labels",
            var.len(),
            niches.len()
        )));
    }
    if var.is_empty() {
        return Ok(NicheComposition {
            phenotypes: Vec::new(),
            niches: Vec::new(),
            counts: Vec::new(),
        });
    }

    let phenotypes: Vec<String> = var
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let niche_ids: Vec<u32> = niches
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let phenotype_index: BTreeMap<&str, usize> = phenotypes
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_str(), i))
        .collect();
    let niche_index: BTreeMap<u32, usize> =
        niche_ids.iter().enumerate().map(|(i, n)| (*n, i)).collect();

    let n_niches = niche_ids.len();
    let mut counts = vec![0.0f64; phenotypes.len() * n_niches];
    for (phenotype, niche) in var.iter().zip(niches) {
        let row = phenotype_index[phenotype.as_str()];
        let column = niche_index[niche];
        counts[row * n_niches + column] += 1.0;
    }

    apply_normalisation(
        &mut counts,
        phenotypes.len(),
        n_niches,
        normalize,
        var.len(),
    );

    Ok(NicheComposition {
        phenotypes,
        niches: niche_ids,
        counts,
    })
}

fn apply_normalisation(
    counts: &mut [f64],
    n_phenotypes: usize,
    n_niches: usize,
    normalize: Normalize,
    n_cells: usize,
) {
    match normalize {
        Normalize::None => {}
        Normalize::Total => {
            let total = n_cells as f64;
            if total > 0.0 {
                counts.iter_mut().for_each(|v| *v /= total);
            }
        }
        Normalize::Obs => normalise_rows(counts, n_phenotypes, n_niches),
        Normalize::Niche => normalise_columns(counts, n_phenotypes, n_niches),
        Normalize::NicheAndObs => {
            normalise_rows(counts, n_phenotypes, n_niches);
            normalise_columns(counts, n_phenotypes, n_niches);
        }
        Normalize::Clr => {
            // `transform_clr` works row by row on a nested matrix.
            let mut rows: Vec<Vec<f64>> = counts
                .chunks(n_niches)
                .map(|chunk| chunk.to_vec())
                .collect();
            transform_clr(&mut rows);
            for (target, row) in counts.chunks_mut(n_niches).zip(rows) {
                target.copy_from_slice(&row);
            }
        }
    }
}

/// `counts.div(counts.sum(axis=1), axis=0)` — each phenotype sums to one.
fn normalise_rows(counts: &mut [f64], n_phenotypes: usize, n_niches: usize) {
    for row in 0..n_phenotypes {
        let slice = &mut counts[row * n_niches..(row + 1) * n_niches];
        let total: f64 = slice.iter().sum();
        // A phenotype with no cells stays at zero rather than becoming NaN.
        if total > 0.0 {
            slice.iter_mut().for_each(|v| *v /= total);
        }
    }
}

/// `counts / counts.sum(axis=0)` — each niche sums to one.
fn normalise_columns(counts: &mut [f64], n_phenotypes: usize, n_niches: usize) {
    for column in 0..n_niches {
        let total: f64 = (0..n_phenotypes)
            .map(|row| counts[row * n_niches + column])
            .sum();
        if total > 0.0 {
            for row in 0..n_phenotypes {
                counts[row * n_niches + column] /= total;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn counts_each_pair_once() {
        let composition =
            make_niches_composition(&labels(&["A", "A", "B"]), &[0, 1, 0], Normalize::None)
                .unwrap();
        assert_eq!(composition.get(0, 0), 1.0);
        assert_eq!(composition.get(0, 1), 1.0);
        assert_eq!(composition.get(1, 0), 1.0);
        assert_eq!(composition.get(1, 1), 0.0);
    }

    #[test]
    fn rows_and_columns_are_sorted() {
        let composition =
            make_niches_composition(&labels(&["b", "a"]), &[7, 2], Normalize::None).unwrap();
        assert_eq!(composition.phenotypes, labels(&["a", "b"]));
        assert_eq!(composition.niches, vec![2, 7]);
    }

    #[test]
    fn total_normalisation_sums_to_one() {
        let composition =
            make_niches_composition(&labels(&["A", "B", "B"]), &[0, 0, 1], Normalize::Total)
                .unwrap();
        assert!((composition.counts.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn niche_and_obs_applies_both_in_order() {
        let var = labels(&["A", "A", "B", "B", "B"]);
        let niches = vec![0, 1, 0, 0, 1];

        let both = make_niches_composition(&var, &niches, Normalize::NicheAndObs).unwrap();
        // Every column must sum to one after the second pass.
        for column in 0..both.niches.len() {
            let sum: f64 = (0..both.phenotypes.len())
                .map(|row| both.get(row, column))
                .sum();
            assert!((sum - 1.0).abs() < 1e-12, "column {column} sums to {sum}");
        }
    }

    #[test]
    fn normalisation_names_match_the_configuration() {
        assert_eq!(Normalize::parse("clr"), Normalize::Clr);
        assert_eq!(Normalize::parse("niche&obs"), Normalize::NicheAndObs);
        assert_eq!(Normalize::parse("total"), Normalize::Total);
        assert_eq!(Normalize::Clr.as_str(), "clr");
    }

    #[test]
    fn a_length_mismatch_is_reported() {
        assert!(make_niches_composition(&labels(&["A"]), &[0, 1], Normalize::Total).is_err());
    }

    #[test]
    fn an_empty_input_yields_an_empty_matrix() {
        let composition = make_niches_composition(&[], &[], Normalize::Clr).unwrap();
        assert!(composition.counts.is_empty());
    }
}
