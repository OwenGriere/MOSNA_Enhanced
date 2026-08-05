//! Agreement with the implementation this port replaces.
//!
//! # Why this file exists
//!
//! Every other test in this workspace answers "is this code consistent with
//! itself?" — the unit tests against each function's definition, the property
//! tests against its invariants, the golden bench against yesterday's output.
//! None of them answers "does this code compute the same thing as the Python it
//! replaces?", which is the entire point of the project.
//!
//! That gap let two defects live in the tree undetected: the Delaunay trimming
//! rule was read off the wrong default, and the mixing matrix's column names
//! were generated in the opposite triangle from its values. Both were
//! *documented* in comments that reasoned they were harmless. Neither was ever
//! executed against the reference.
//!
//! # What is compared, and to what tolerance
//!
//! `test/parity/reference.json` holds the output of the Python implementation
//! on `test/patient_folder`, recorded once. The tolerances below are not
//! decoration — each says what kind of agreement is expected and why:
//!
//! | Quantity | Tolerance | Why |
//! |---|---|---|
//! | Edge set | exact | both sides deterministic |
//! | Mixing matrix, assortativity, proportions | 1e-12 | measured at 1e-17 |
//! | Column names | exact | a name is a label, not a number |
//! | NAS features | 1e-12 | deterministic aggregation |
//!
//! Nothing stochastic is compared here. UMAP and the clusterers are
//! reimplemented, and the Python reference is not even reproducible against
//! itself — `get_reducer` passes `random_state=None`. Comparing niche labels
//! would be comparing two different random draws.

use std::collections::BTreeSet;
use std::path::PathBuf;

use mosna_core::assortativity::mixmat_columns::attributes_pairs;
use mosna_core::assortativity::{attribute_ac, mixing_matrix, mixmat_to_columns};
use mosna_core::geometry::{build_delaunay, link_solitaries, DelaunayTrim, LinkMethod};
use mosna_core::nas::make_features_nas::make_features_nas;
use mosna_core::nas::onehot::one_hot;
use mosna_io::read::get_opener::{read_table, Extension};

/// Exact agreement is expected of every deterministic numeric stage; this is
/// three orders of magnitude above the residual actually observed.
const TOLERANCE: f64 = 1e-12;

fn repo_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `crates/mosna-pipeline`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is reachable from the manifest")
}

fn reference() -> serde_json::Value {
    let path = repo_root().join("test/parity/reference.json");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).expect("the reference is valid JSON")
}

fn patients() -> Vec<String> {
    reference()["samples"]
        .as_object()
        .expect("samples is an object")
        .keys()
        .cloned()
        .collect()
}

/// The coordinates and phenotypes of one reference sample.
fn sample(patient: &str) -> (Vec<[f64; 2]>, Vec<Option<String>>) {
    let path = repo_root().join(format!("test/patient_folder/nodes_patient-{patient}.csv"));
    let table = read_table(&path, Extension::Csv).expect("the fixture reads");
    let coords = table.coords("X", "Y").expect("coordinate columns");
    let phenotypes = table
        .opt_string_column("phenotype")
        .expect("phenotype column");
    (coords, phenotypes)
}

/// The reference edge set, as unordered canonical pairs.
fn reference_edges(patient: &str) -> BTreeSet<(u32, u32)> {
    reference()["samples"][patient]["edges"]
        .as_array()
        .expect("edges is an array")
        .iter()
        .map(|pair| {
            let a = pair[0].as_u64().unwrap() as u32;
            let b = pair[1].as_u64().unwrap() as u32;
            (a.min(b), a.max(b))
        })
        .collect()
}

fn floats(value: &serde_json::Value) -> Vec<f64> {
    value
        .as_array()
        .expect("an array of numbers")
        .iter()
        .map(|v| v.as_f64().expect("a number"))
        .collect()
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("an array of strings")
        .iter()
        .map(|v| v.as_str().expect("a string").to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Step 1 — the network
// ---------------------------------------------------------------------------

/// The reconstructed network must be the reference's network, edge for edge.
///
/// This is the foundation: steps 2 and 3 read this graph, so any disagreement
/// here makes every downstream comparison meaningless.
#[test]
fn step_one_rebuilds_the_reference_network() {
    for patient in patients() {
        let (coords, _) = sample(&patient);
        let expected = reference_edges(&patient);

        let pairs = build_delaunay(&coords, DelaunayTrim::default()).expect("triangulation");
        let pairs = link_solitaries(&coords, &pairs, LinkMethod::Delaunay, 3).expect("linking");
        let got: BTreeSet<(u32, u32)> = pairs.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();

        let missing = expected.difference(&got).count();
        let extra = got.difference(&expected).count();
        assert!(
            missing == 0 && extra == 0,
            "patient-{patient}: {} edges against {} expected — {missing} missing, {extra} extra",
            got.len(),
            expected.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Step 2 — assortativity
// ---------------------------------------------------------------------------

/// Build the per-sample assignment vector the way the reference does: the
/// vocabulary is `np.unique` of that sample's phenotypes, which is sorted.
fn assignments(phenotypes: &[Option<String>]) -> (Vec<String>, Vec<Option<u32>>) {
    let vocabulary: Vec<String> = phenotypes
        .iter()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let assignments = phenotypes
        .iter()
        .map(|p| {
            p.as_ref()
                .and_then(|label| vocabulary.iter().position(|v| v == label))
                .map(|i| i as u32)
        })
        .collect();
    (vocabulary, assignments)
}

/// The mixing matrix and the assortativity coefficient must match to machine
/// precision. They already did when this harness was written — this test is
/// here to keep it that way.
#[test]
fn step_two_reproduces_the_mixing_matrix() {
    for patient in patients() {
        let reference = reference();
        let expected_values = floats(&reference["samples"][&patient]["mixmat_values"]);
        let expected_assort = reference["samples"][&patient]["assort"]
            .as_f64()
            .expect("assort is a number");

        let (coords, phenotypes) = sample(&patient);
        let (vocabulary, codes) = assignments(&phenotypes);
        let pairs = link_solitaries(
            &coords,
            &build_delaunay(&coords, DelaunayTrim::default()).unwrap(),
            LinkMethod::Delaunay,
            3,
        )
        .unwrap();

        let mixmat = mixing_matrix(&codes, &pairs, vocabulary.len(), true, true);
        let got_values = mixmat_to_columns(&mixmat);
        let got_assort = attribute_ac(&mixmat);

        assert!(
            (got_assort - expected_assort).abs() < TOLERANCE,
            "patient-{patient}: assort {got_assort} against {expected_assort}"
        );
        assert_eq!(got_values.len(), expected_values.len());
        for (i, (got, want)) in got_values.iter().zip(&expected_values).enumerate() {
            assert!(
                (got - want).abs() < TOLERANCE,
                "patient-{patient}: mixing matrix element {i} is {got}, expected {want}"
            );
        }
    }
}

/// Every column name must designate the element whose value it carries.
///
/// This is the property that was never stated. `mixmat_to_columns` and
/// `attributes_pairs` were each tested in isolation and both passed, while
/// walking the matrix in opposite triangles — so `net_stat.csv` reported one
/// phenotype pair's value under another pair's name, silently, for every
/// dataset with three or more phenotypes.
#[test]
fn step_two_names_its_columns_the_way_the_reference_does() {
    for patient in patients() {
        let reference = reference();
        let expected = strings(&reference["samples"][&patient]["mixmat_columns"]);
        let (_, phenotypes) = sample(&patient);
        let (vocabulary, _) = assignments(&phenotypes);

        let got = attributes_pairs(&vocabulary, "", " - ", "");
        assert_eq!(
            got, expected,
            "patient-{patient}: the mixing matrix columns are not named as the reference names them"
        );
    }
}

/// The names and the values must walk the matrix in the same order, whatever
/// that order is. Checked against the reference, which is self-consistent.
#[test]
fn step_two_pairs_each_name_with_its_own_value() {
    for patient in patients() {
        let reference = reference();
        let names = strings(&reference["samples"][&patient]["mixmat_columns"]);
        let values = floats(&reference["samples"][&patient]["mixmat_values"]);

        let (coords, phenotypes) = sample(&patient);
        let (vocabulary, codes) = assignments(&phenotypes);
        let pairs = link_solitaries(
            &coords,
            &build_delaunay(&coords, DelaunayTrim::default()).unwrap(),
            LinkMethod::Delaunay,
            3,
        )
        .unwrap();
        let mixmat = mixing_matrix(&codes, &pairs, vocabulary.len(), true, true);

        let got_names = attributes_pairs(&vocabulary, "", " - ", "");
        let got_values = mixmat_to_columns(&mixmat);

        for (name, value) in got_names.iter().zip(&got_values) {
            let position = names
                .iter()
                .position(|n| n == name)
                .unwrap_or_else(|| panic!("patient-{patient}: `{name}` is not a reference column"));
            assert!(
                (value - values[position]).abs() < TOLERANCE,
                "patient-{patient}: `{name}` carries {value}, the reference puts {} there",
                values[position]
            );
        }
    }
}

/// Phenotype proportions, which the reference writes as `% <phenotype>`.
#[test]
fn step_two_reproduces_the_phenotype_proportions() {
    for patient in patients() {
        let reference = reference();
        let expected = reference["samples"][&patient]["percent"]
            .as_object()
            .expect("percent is an object")
            .clone();

        let (_, phenotypes) = sample(&patient);
        let n = phenotypes.len() as f64;
        for (label, want) in expected {
            let count = phenotypes
                .iter()
                .filter(|p| p.as_deref() == Some(label.as_str()))
                .count() as f64;
            let got = count / n;
            let want = want.as_f64().unwrap();
            assert!(
                (got - want).abs() < TOLERANCE,
                "patient-{patient}: % {label} is {got}, expected {want}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Step 3 — the deterministic stage
// ---------------------------------------------------------------------------

/// The neighbourhood aggregation must match the reference exactly.
///
/// Only this stage of step 3 is comparable: what follows it — UMAP, then a
/// clusterer — is reimplemented, and the Python reference is not reproducible
/// against itself there.
#[test]
fn step_three_reproduces_the_neighbourhood_features() {
    let reference = reference();
    let vocabulary = strings(&reference["nas"]["vocabulary"]);
    let stat_names: Vec<String> = strings(&reference["nas"]["stat_names"]);

    for patient in patients() {
        let expected_columns = strings(&reference["samples"][&patient]["nas_columns"]);
        let expected_sums = floats(&reference["samples"][&patient]["nas_col_sums"]);
        let expected_rows: Vec<Vec<f64>> = reference["samples"][&patient]["nas_first_rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(floats)
            .collect();

        let (coords, phenotypes) = sample(&patient);
        let pairs = link_solitaries(
            &coords,
            &build_delaunay(&coords, DelaunayTrim::default()).unwrap(),
            LinkMethod::Delaunay,
            3,
        )
        .unwrap();

        let x = one_hot(&phenotypes, &vocabulary);
        let features =
            make_features_nas(&x, coords.len(), &pairs, 1, &vocabulary, &stat_names, " ");

        assert_eq!(
            features.column_names, expected_columns,
            "patient-{patient}: NAS column layout"
        );

        let width = features.n_columns();
        for (column, want) in expected_sums.iter().enumerate() {
            let got: f64 = (0..coords.len()).map(|row| features.row(row)[column]).sum();
            assert!(
                (got - want).abs() < TOLERANCE * coords.len() as f64,
                "patient-{patient}: column {column} sums to {got}, expected {want}"
            );
        }

        for (row, want) in expected_rows.iter().enumerate() {
            let got = features.row(row);
            assert_eq!(got.len(), width);
            for (column, want) in want.iter().enumerate() {
                assert!(
                    (got[column] - want).abs() < TOLERANCE,
                    "patient-{patient}: NAS[{row}][{column}] is {}, expected {want}",
                    got[column]
                );
            }
        }
    }
}
