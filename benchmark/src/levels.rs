//! The three levels of the bench.
//!
//! Each takes a cohort specification and returns something a report can print.
//! Nothing here writes to the disk except by way of the caller: the levels are
//! functions over data, so they are testable.

use std::collections::BTreeMap;

use mosna_core::assortativity::attribute_ac::attribute_ac;
use mosna_core::assortativity::mixing_matrix::mixing_matrix;
use mosna_core::assortativity::sample_assort_mixmat::sample_assort_mixmat;
use mosna_core::clustering::gmm::{gaussian_mixture, GmmParams};
use mosna_core::clustering::leiden::leiden;
use mosna_core::geometry::build_delaunay::{build_delaunay, DelaunayTrim};
use mosna_core::geometry::link_solitaries::{link_solitaries, LinkMethod};
use mosna_core::nas::make_features_nas::make_features_nas;
use mosna_core::nas::onehot::one_hot;
use mosna_core::reduction::umap::{umap, UmapParams};

use crate::agreement::{adjusted_rand_index, knn_overlap, normalized_mutual_information};
use crate::cohort::{CohortSpec, Tissue};
use crate::fingerprint::Fingerprint;

/// The neighbourhood a cell is described by, throughout the bench.
const ORDER: usize = 1;
/// Minimum degree the network guarantees, as the shipped configuration does.
const MIN_NEIGHBORS: usize = 3;
/// What counts as the same number across two parallel reductions.
///
/// Three orders of magnitude above the residual actually observed (~1e-15), and
/// far below anything that could change a conclusion.
const FLOAT_TOLERANCE: f64 = 1e-12;

/// Everything the deterministic core computes on one cohort, as one record.
///
/// This is level 1. Every stage here is a pure function of the input: same
/// tissue in, same numbers out, on any machine, with any thread count. What it
/// catches is numerical drift — a refactor that changes the fifteenth digit,
/// which is exactly the kind of change nobody notices until results move.
pub fn level_1_golden(spec: &CohortSpec) -> Fingerprint {
    let mut fingerprint = Fingerprint::default();
    let phenotypes = vocabulary(spec);

    for index in 0..spec.n_samples {
        let tissue = Tissue::generate(spec, index);
        let stage = |name: &str| format!("sample_{index}/{name}");

        // -- geometry -------------------------------------------------------
        let raw = build_delaunay(&tissue.coords, DelaunayTrim::default())
            .expect("a cohort always has enough points");
        fingerprint.pairs(&stage("delaunay"), &raw);

        let edges = link_solitaries(&tissue.coords, &raw, LinkMethod::Delaunay, MIN_NEIGHBORS)
            .expect("relinking cannot fail on a valid graph");
        fingerprint.pairs(&stage("edges"), &edges);

        // The degree distribution is what the min-neighbours guarantee is
        // about, and it moves for reasons a hash of the edge set would not
        // explain.
        let mut degrees = vec![0u32; tissue.coords.len()];
        for &(a, b) in &edges {
            degrees[a as usize] += 1;
            degrees[b as usize] += 1;
        }
        fingerprint.floats(
            &stage("degree_summary"),
            &[
                degrees.iter().map(|&d| d as f64).sum::<f64>() / degrees.len() as f64,
                *degrees.iter().min().unwrap() as f64,
                *degrees.iter().max().unwrap() as f64,
            ],
        );

        // -- neighbourhood features -----------------------------------------
        let (features, n_rows, _) = neighbourhood_features(&tissue, &edges, &phenotypes);
        // The whole feature matrix is far too large for git; its column means
        // are not, and they move whenever the aggregation does.
        fingerprint.floats(&stage("nas_column_means"), &column_means(&features, n_rows));

        // -- assortativity ---------------------------------------------------
        let assignments = assignments_of(&tissue, &phenotypes);
        let mixmat = mixing_matrix(&assignments, &edges, phenotypes.len(), true, true);
        fingerprint.floats(&stage("mixing_matrix"), &mixmat.values);
        fingerprint.floats(&stage("assortativity"), &[attribute_ac(&mixmat)]);
    }

    fingerprint
}

/// Level 2: the seeded stochastic parts must give the same answer twice.
///
/// Every stochastic algorithm in the port takes a seed and derives a per-item
/// stream from it, so *which* random draws are made does not depend on how many
/// threads happened to run. That is a claim, and this is the test of it: the
/// same work is run twice, the second time with the thread pool restricted to
/// one, and the two fingerprints are compared.
///
/// # Why the labels are exact and the numbers are not
///
/// A partition is exact: a cell either landed in the same niche or it did not,
/// and there is no such thing as nearly the same label. So `gmm_labels` and
/// `leiden_labels` are compared bit for bit.
///
/// The z-scores are not, and cannot be. Their null distribution is summed by a
/// parallel reduction, and a parallel reduction associates its additions
/// differently depending on how the work was split — `(a+b)+c` against
/// `a+(b+c)`. Floating-point addition is not associative, so the last bits
/// move. The permutations themselves are identical, which is the part that
/// matters; the residual is around 1e-15 relative, and demanding zero there
/// would turn this level into a random alarm.
///
/// Making even that exact is possible — fold into a fixed number of chunks
/// rather than however many rayon chose — at the cost of holding those chunk
/// accumulators in memory. Not worth it for 1e-15.
pub fn level_2_reproducibility(spec: &CohortSpec) -> Reproducibility {
    let first = seeded_run(spec);

    // A single-threaded pool: if any result depends on the scheduling, this is
    // where it shows.
    let second = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("a single-threaded pool")
        .install(|| seeded_run(spec));

    Reproducibility {
        differences: first.differences(&second, FLOAT_TOLERANCE),
    }
}

/// The outcome of level 2.
#[derive(Debug, Clone, Default)]
pub struct Reproducibility {
    /// Empty when the two runs agreed: labels bit for bit, numbers within the
    /// tolerance a parallel reduction leaves behind.
    pub differences: Vec<String>,
}

impl Reproducibility {
    pub fn is_reproducible(&self) -> bool {
        self.differences.is_empty()
    }
}

/// One pass over the seeded stochastic stages.
fn seeded_run(spec: &CohortSpec) -> Fingerprint {
    let mut fingerprint = Fingerprint::default();
    let phenotypes = vocabulary(spec);

    // Only the first sample: level 2 is about determinism, not coverage, and
    // the stochastic code paths are the same from one sample to the next.
    let tissue = Tissue::generate(spec, 0);
    let edges = network_of(&tissue);
    let assignments = assignments_of(&tissue, &phenotypes);

    // The permutation null. Its seed is fixed inside the core, precisely so
    // this is reproducible; the Python cannot make the same promise.
    let stats = sample_assort_mixmat(&assignments, &edges, &phenotypes, "sample_0", 32);
    fingerprint.floats("assortativity_zscores", &stats.values);

    let (features, n_rows, n_features) = neighbourhood_features(&tissue, &edges, &phenotypes);

    let embedding = embed(&features, n_rows, n_features);
    fingerprint.floats("umap_embedding", &embedding);

    let gmm = cluster(&embedding, n_rows, spec.n_niches);
    fingerprint.labels("gmm_labels", &gmm);

    let graph: Vec<(usize, usize, f64)> = edges
        .iter()
        .map(|&(a, b)| (a as usize, b as usize, 1.0))
        .collect();
    fingerprint.labels("leiden_labels", &leiden(n_rows, &graph, 1.0, 42));

    fingerprint
}

/// Level 3: do the niches the pipeline finds match the ones that were planted?
///
/// The one question the other two levels cannot answer. Level 1 says the
/// numbers have not moved and level 2 says they are reproducible — both would
/// still hold if the analysis were reproducibly wrong. Here the tissue is
/// generated with known niches, so there is a ground truth to score against.
pub fn level_3_recovery(spec: &CohortSpec) -> Recovery {
    let phenotypes = vocabulary(spec);
    let tissue = Tissue::generate(spec, 0);
    let edges = network_of(&tissue);

    let (features, n_rows, n_features) = neighbourhood_features(&tissue, &edges, &phenotypes);
    let embedding = embed(&features, n_rows, n_features);
    let found = cluster(&embedding, n_rows, spec.n_niches);

    // Sub-sampled: the overlap is quadratic in the point count, and a few
    // hundred points already answer the question.
    let step = (n_rows / 400).max(1);
    let sampled: Vec<usize> = (0..n_rows).step_by(step).collect();
    let take = |data: &[f64], dim: usize| -> Vec<f64> {
        sampled
            .iter()
            .flat_map(|&row| data[row * dim..(row + 1) * dim].to_vec())
            .collect()
    };

    Recovery {
        adjusted_rand: adjusted_rand_index(&found, &tissue.niches),
        mutual_information: normalized_mutual_information(&found, &tissue.niches),
        neighbourhood_overlap: knn_overlap(
            &take(&features, n_features),
            &take(&embedding, 2),
            sampled.len(),
            n_features,
            2,
            15,
        ),
        n_found: distinct(&found),
        n_planted: spec.n_niches,
    }
}

/// The outcome of level 3.
#[derive(Debug, Clone)]
pub struct Recovery {
    /// Agreement between the niches found and the niches planted.
    pub adjusted_rand: f64,
    pub mutual_information: f64,
    /// Proportion of neighbours the projection kept.
    pub neighbourhood_overlap: f64,
    pub n_found: usize,
    pub n_planted: usize,
}

// ---------------------------------------------------------------------------
// Shared steps
// ---------------------------------------------------------------------------

/// The phenotype vocabulary, in the order the one-hot encoding uses.
fn vocabulary(spec: &CohortSpec) -> Vec<String> {
    (0..spec.n_phenotypes)
        .map(|index| format!("pheno_{index}"))
        .collect()
}

/// The network of a tissue, as step one builds it.
fn network_of(tissue: &Tissue) -> Vec<(u32, u32)> {
    let raw = build_delaunay(&tissue.coords, DelaunayTrim::default()).expect("enough points");
    link_solitaries(&tissue.coords, &raw, LinkMethod::Delaunay, MIN_NEIGHBORS)
        .expect("relinking cannot fail")
}

/// Each cell as an index into the vocabulary.
fn assignments_of(tissue: &Tissue, phenotypes: &[String]) -> Vec<Option<u32>> {
    let index: BTreeMap<&str, u32> = phenotypes
        .iter()
        .enumerate()
        .map(|(position, name)| (name.as_str(), position as u32))
        .collect();
    tissue
        .phenotypes
        .iter()
        .map(|name| index.get(name.as_str()).copied())
        .collect()
}

/// The neighbourhood description of every cell: `(values, rows, columns)`.
fn neighbourhood_features(
    tissue: &Tissue,
    edges: &[(u32, u32)],
    phenotypes: &[String],
) -> (Vec<f64>, usize, usize) {
    let labels: Vec<Option<String>> = tissue.phenotypes.iter().cloned().map(Some).collect();
    let encoded = one_hot(&labels, phenotypes);

    let nas = make_features_nas(
        &encoded,
        tissue.coords.len(),
        edges,
        ORDER,
        phenotypes,
        &["mean".to_string(), "std".to_string()],
        " ",
    );
    let columns = nas.column_names.len();
    (nas.values, nas.n_rows, columns)
}

/// The projection, with the settings the bench holds fixed everywhere.
fn embed(features: &[f64], n_rows: usize, n_features: usize) -> Vec<f64> {
    umap(
        features,
        n_rows,
        n_features,
        &UmapParams {
            n_components: 2,
            n_neighbors: 15,
            seed: 42,
            ..Default::default()
        },
    )
    .expect("the cohort is large enough to embed")
}

/// The niches, from the projection.
fn cluster(embedding: &[f64], n_rows: usize, n_clusters: usize) -> Vec<u32> {
    gaussian_mixture(
        embedding,
        n_rows,
        2,
        &GmmParams {
            n_clusters,
            seed: 42,
            ..Default::default()
        },
    )
    .expect("a fitted mixture")
    .labels
}

fn column_means(values: &[f64], n_rows: usize) -> Vec<f64> {
    if n_rows == 0 {
        return Vec::new();
    }
    let width = values.len() / n_rows;
    (0..width)
        .map(|column| {
            (0..n_rows)
                .map(|row| values[row * width + column])
                .sum::<f64>()
                / n_rows as f64
        })
        .collect()
}

fn distinct(labels: &[u32]) -> usize {
    let mut seen: Vec<u32> = labels.to_vec();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> CohortSpec {
        CohortSpec {
            n_samples: 1,
            cells_per_sample: 300,
            n_phenotypes: 4,
            n_niches: 3,
            seed: 1,
        }
    }

    #[test]
    fn the_golden_record_covers_every_deterministic_stage() {
        let fingerprint = level_1_golden(&small());
        let stages: Vec<&str> = fingerprint.stages().collect();
        for expected in [
            "sample_0/delaunay",
            "sample_0/edges",
            "sample_0/degree_summary",
            "sample_0/nas_column_means",
            "sample_0/mixing_matrix",
            "sample_0/assortativity",
        ] {
            assert!(
                stages.contains(&expected),
                "missing {expected} in {stages:?}"
            );
        }
    }

    /// The claim level 1 rests on: the deterministic core is deterministic.
    #[test]
    fn the_golden_record_is_the_same_twice() {
        let spec = small();
        assert!(level_1_golden(&spec)
            .differences(&level_1_golden(&spec), 0.0)
            .is_empty());
    }

    #[test]
    fn the_column_means_average_each_column() {
        let values = [1.0, 10.0, 3.0, 20.0];
        assert_eq!(column_means(&values, 2), vec![2.0, 15.0]);
    }

    #[test]
    fn the_vocabulary_matches_what_the_generator_writes() {
        let spec = small();
        let tissue = Tissue::generate(&spec, 0);
        let phenotypes = vocabulary(&spec);
        assert!(
            tissue.phenotypes.iter().all(|p| phenotypes.contains(p)),
            "the generator produced a phenotype outside the vocabulary"
        );
    }

    #[test]
    fn every_cell_is_assigned_to_a_known_phenotype() {
        let spec = small();
        let tissue = Tissue::generate(&spec, 0);
        let assignments = assignments_of(&tissue, &vocabulary(&spec));
        assert!(assignments.iter().all(Option::is_some));
    }

    #[test]
    fn the_counted_niches_are_the_distinct_ones() {
        assert_eq!(distinct(&[3, 1, 1, 3, 0]), 3);
        assert_eq!(distinct(&[]), 0);
    }
}
