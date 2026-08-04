//! Tests of the synthetic tissue generator, written before the implementation.
//!
//! A benchmark is only worth its data. Uniform noise would make the niche
//! analysis measure nothing — every neighbourhood would have the same
//! composition, so any partition would be as good as any other and the
//! agreement metrics would be meaningless. The generator therefore has to
//! produce tissue with real spatial structure, and these tests are what pin
//! that down.

use mosna_bench::cohort::{write_cohort, CohortSpec, Tissue};

fn spec(cells: usize) -> CohortSpec {
    CohortSpec {
        n_samples: 2,
        cells_per_sample: cells,
        n_phenotypes: 6,
        n_niches: 4,
        seed: 7,
    }
}

// ---------------------------------------------------------------------------
// Reproducibility
// ---------------------------------------------------------------------------

/// A benchmark that generates different data on each run measures noise.
#[test]
fn the_same_seed_gives_exactly_the_same_tissue() {
    let a = Tissue::generate(&spec(500), 0);
    let b = Tissue::generate(&spec(500), 0);

    assert_eq!(a.coords, b.coords);
    assert_eq!(a.phenotypes, b.phenotypes);
    assert_eq!(a.niches, b.niches);
}

#[test]
fn a_different_seed_gives_different_tissue() {
    let mut other = spec(500);
    other.seed = 8;
    assert_ne!(
        Tissue::generate(&spec(500), 0).coords,
        Tissue::generate(&other, 0).coords
    );
}

/// Two samples of one cohort must not be copies of each other.
#[test]
fn each_sample_of_a_cohort_is_its_own_tissue() {
    let spec = spec(500);
    assert_ne!(
        Tissue::generate(&spec, 0).coords,
        Tissue::generate(&spec, 1).coords
    );
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn the_requested_size_is_produced() {
    let tissue = Tissue::generate(&spec(1234), 0);
    assert_eq!(tissue.coords.len(), 1234);
    assert_eq!(tissue.phenotypes.len(), 1234);
    assert_eq!(tissue.niches.len(), 1234);
}

#[test]
fn every_phenotype_of_the_vocabulary_occurs() {
    let tissue = Tissue::generate(&spec(3000), 0);
    let mut seen: Vec<&str> = tissue.phenotypes.iter().map(String::as_str).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 6, "got {seen:?}");
}

#[test]
fn every_niche_is_populated() {
    let tissue = Tissue::generate(&spec(3000), 0);
    for niche in 0..4u32 {
        let count = tissue.niches.iter().filter(|&&n| n == niche).count();
        assert!(count > 0, "niche {niche} is empty");
    }
}

// ---------------------------------------------------------------------------
// The structure that makes the measurement meaningful
// ---------------------------------------------------------------------------

/// Cells of one niche sit together. Without this, there is no spatial signal
/// for the niche analysis to find, and level 3 measures nothing.
#[test]
fn a_niche_is_spatially_compact() {
    let tissue = Tissue::generate(&spec(2000), 0);

    let distance = |a: usize, b: usize| {
        let (p, q) = (tissue.coords[a], tissue.coords[b]);
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
    };

    let mut within = (0.0, 0usize);
    let mut between = (0.0, 0usize);
    for a in (0..tissue.coords.len()).step_by(7) {
        for b in (0..tissue.coords.len()).step_by(11) {
            if a == b {
                continue;
            }
            let target = if tissue.niches[a] == tissue.niches[b] {
                &mut within
            } else {
                &mut between
            };
            target.0 += distance(a, b);
            target.1 += 1;
        }
    }

    let within = within.0 / within.1 as f64;
    let between = between.0 / between.1 as f64;
    assert!(
        within < between * 0.7,
        "niches are not compact: {within:.2} within vs {between:.2} between"
    );
}

/// Two niches must differ in what they are made of, or the composition
/// heatmap — and the clustering that produces it — has nothing to separate.
#[test]
fn niches_differ_in_composition() {
    let spec = spec(4000);
    let tissue = Tissue::generate(&spec, 0);

    let composition = |niche: u32| {
        let mut counts = vec![0.0; spec.n_phenotypes];
        let mut total = 0.0;
        for (index, &n) in tissue.niches.iter().enumerate() {
            if n == niche {
                let phenotype = &tissue.phenotypes[index];
                let position: usize = phenotype
                    .trim_start_matches("pheno_")
                    .parse()
                    .expect("a numbered phenotype");
                counts[position] += 1.0;
                total += 1.0;
            }
        }
        counts.iter().map(|c| c / total).collect::<Vec<f64>>()
    };

    let a = composition(0);
    let b = composition(1);
    let divergence: f64 = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum();
    assert!(
        divergence > 0.3,
        "niches 0 and 1 have near-identical composition (L1 = {divergence:.3})"
    );
}

// ---------------------------------------------------------------------------
// On disk
// ---------------------------------------------------------------------------

/// The cohort must be readable by the pipelines, in the layout they expect.
#[test]
fn a_written_cohort_is_discoverable() {
    let dir = tempfile::tempdir().unwrap();
    write_cohort(&spec(200), dir.path()).unwrap();

    let files = mosna_io::find_sample(dir.path(), "parquet", "patient", Some("sample")).unwrap();
    assert_eq!(files.len(), 2, "one nodes file per sample");

    let nodes = mosna_io::read::read_parquet::read_parquet(&files[0]).unwrap();
    assert_eq!(nodes.n_rows(), 200);
    for column in ["X_position", "Y_position", "Cluster"] {
        assert!(nodes.has_column(column), "missing column {column}");
    }
}

/// The ground-truth niche travels with the data: level 3 compares the niches
/// the pipeline finds against the ones the generator planted.
#[test]
fn the_true_niche_is_written_alongside() {
    let dir = tempfile::tempdir().unwrap();
    write_cohort(&spec(200), dir.path()).unwrap();

    let files = mosna_io::find_sample(dir.path(), "parquet", "patient", Some("sample")).unwrap();
    let nodes = mosna_io::read::read_parquet::read_parquet(&files[0]).unwrap();
    assert!(
        nodes.has_column("true_niche"),
        "the planted niche must be recoverable for scoring"
    );
}

/// Writing twice must produce byte-identical files, or a golden fingerprint
/// taken on Monday fails on Tuesday for no reason.
#[test]
fn writing_is_reproducible() {
    let spec = spec(300);
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    write_cohort(&spec, one.path()).unwrap();
    write_cohort(&spec, two.path()).unwrap();

    let name = "nodes_patient-1_sample-1.parquet";
    assert_eq!(
        std::fs::read(one.path().join(name)).unwrap(),
        std::fs::read(two.path().join(name)).unwrap()
    );
}
