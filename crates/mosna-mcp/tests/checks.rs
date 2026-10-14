//! The checks on a small cohort written to disk, one defect per file.

use std::path::Path;
use std::sync::Arc;

use arrow_array::{ArrayRef, Float64Array, Int64Array, StringArray};
use serde_json::{json, Value};

use mosna_io::{write_parquet, Table};
use mosna_mcp::check::naming::Naming;
use mosna_mcp::check::network::check_network;
use mosna_mcp::check::nodes::{check_nodes, NodesSpec};
use mosna_mcp::protocol::handle_line;
use mosna_mcp::Server;

fn spec() -> NodesSpec {
    NodesSpec {
        naming: Naming {
            extension: "parquet".into(),
            patient_column: "patient".into(),
            sample_column: Some("sample".into()),
        },
        x_column: "x".into(),
        y_column: "y".into(),
        phenotype_column: "cell_type".into(),
        min_neighbors: 3,
    }
}

fn write(dir: &Path, name: &str, xs: Vec<Option<f64>>, ys: Vec<f64>, phenotypes: ArrayRef) {
    let table = Table::from_columns(vec![
        ("x".into(), Arc::new(Float64Array::from(xs)) as ArrayRef),
        ("y".into(), Table::f64_array(ys)),
        ("cell_type".into(), phenotypes),
    ])
    .unwrap();
    write_parquet(&table, dir.join(name)).unwrap();
}

fn labels(values: &[Option<&str>]) -> ArrayRef {
    Arc::new(StringArray::from(values.to_vec()))
}

fn square() -> (Vec<Option<f64>>, Vec<f64>) {
    (
        vec![Some(0.0), Some(1.0), Some(0.0), Some(1.0)],
        vec![0.0, 0.0, 1.0, 1.0],
    )
}

fn codes(report: &mosna_mcp::report::Report, file: &str) -> Vec<&'static str> {
    report
        .findings
        .iter()
        .filter(|f| f.file.as_deref() == Some(file))
        .map(|f| f.code)
        .collect()
}

#[test]
fn each_defect_is_found_in_its_own_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let (xs, ys) = square();
    let ab = labels(&[Some("a"), Some("b"), Some("a"), Some("b")]);

    write(
        p,
        "nodes_patient-1_sample-1.parquet",
        xs.clone(),
        ys.clone(),
        ab.clone(),
    );
    write(
        p,
        "nodes_patient-2_sample-1.parquet",
        vec![Some(0.0), None, Some(0.0), Some(1.0)],
        ys.clone(),
        ab.clone(),
    );
    write(
        p,
        "nodes_patient-3_sample-1.parquet",
        vec![Some(0.0), Some(1.0), Some(2.0)],
        vec![0.0, 1.0, 2.0],
        labels(&[Some("a"), Some("b"), Some("a")]),
    );
    write(
        p,
        "nodes_patient-4_sample-1.parquet",
        vec![Some(0.0), Some(0.0), Some(0.0), Some(1.0), Some(1.0)],
        vec![0.0, 0.0, 1.0, 0.0, 1.0],
        labels(&[Some("a"), Some("a"), None, Some("b"), Some("b")]),
    );
    write(p, "nodes_patient-5_sample-A_B.parquet", xs, ys, ab);

    let (report, samples) = check_nodes(p, &spec());

    assert!(codes(&report, "nodes_patient-1_sample-1.parquet").is_empty());
    assert_eq!(
        codes(&report, "nodes_patient-2_sample-1.parquet"),
        ["coord-non-finite"]
    );
    assert_eq!(
        codes(&report, "nodes_patient-3_sample-1.parquet"),
        ["collinear"]
    );
    let four = codes(&report, "nodes_patient-4_sample-1.parquet");
    assert!(four.contains(&"duplicate-coordinates"), "{four:?}");
    assert!(four.contains(&"phenotype-null"), "{four:?}");
    assert_eq!(
        codes(&report, "nodes_patient-5_sample-A_B.parquet"),
        ["name-ignored"]
    );
    assert_eq!(samples.as_array().unwrap().len(), 4);
    assert!(!report.is_ok());
}

#[test]
fn integer_and_float_phenotypes_split_the_cohort_in_rust_only() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let (xs, ys) = square();
    write(
        p,
        "nodes_patient-1_sample-1.parquet",
        xs.clone(),
        ys.clone(),
        Arc::new(Int64Array::from(vec![1, 2, 1, 2])),
    );
    write(
        p,
        "nodes_patient-2_sample-1.parquet",
        xs,
        ys,
        Table::f64_array([1.0, 2.0, 1.0, 2.0]),
    );

    let (report, _) = check_nodes(p, &spec());
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == "phenotype-type-mismatch")
        .expect("the mismatch is reported");
    assert_eq!(finding.affects.as_str(), "rust");
}

#[test]
fn an_edge_past_the_last_row_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let (xs, ys) = square();
    write(
        p,
        "nodes_patient-1_sample-1.parquet",
        xs,
        ys,
        labels(&[Some("a"), Some("b"), Some("a"), Some("b")]),
    );
    write_parquet(
        &Table::from_edges(&[(0, 1), (1, 2), (2, 7)]).unwrap(),
        p.join("edges_patient-1_sample-1.parquet"),
    )
    .unwrap();

    let naming = spec().naming;
    let (report, rows) = check_network(p, &naming, Some("cell_type"));
    assert!(report
        .findings
        .iter()
        .any(|f| f.code == "edge-out-of-range"));
    assert_eq!(rows[0]["edges"], 2);
    assert_eq!(rows[0]["isolated"], 1);
}

/// The whole path a client takes: JSON-RPC in, JSON report out.
#[test]
fn check_nodes_answers_over_the_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let (xs, ys) = square();
    write(
        p,
        "nodes_patient-1_sample-1.parquet",
        xs,
        ys,
        labels(&[Some("a"), Some("b"), Some("a"), Some("b")]),
    );

    let request = json!({
        "jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": { "name": "check_nodes", "arguments": {
            "working_dir": p, "nodes_dir": ".", "extension": "parquet",
            "patient_column": "patient", "sample_column": "sample",
            "x_column": "x", "y_column": "y", "phenotype_column": "cell_type",
        }},
    });
    let response = handle_line(&Server::from_env(), &request.to_string()).unwrap();
    assert_eq!(response["result"]["isError"], false);
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    let body: Value = serde_json::from_str(text).unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["sample_count"], 1);
}

/// The same partition under other names is a perfect agreement.
#[test]
fn renamed_niches_agree_perfectly() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let niches = |dir: &Path, column: &str, values: [&str; 4]| {
        let table =
            Table::from_columns(vec![(column.into(), Table::string_array(values))]).unwrap();
        write_parquet(&table, dir.join("nodes_patient-1_sample-1.parquet")).unwrap();
    };
    niches(a.path(), "niches_1", ["0", "0", "1", "1"]);
    niches(b.path(), "niches", ["x", "x", "y", "y"]);

    let naming = spec().naming;
    let result = mosna_mcp::compare::compare_partitions(
        (a.path(), &naming, "niches_1"),
        (b.path(), &naming, "niches"),
        true,
    )
    .unwrap();
    assert_eq!(result["ari"], 1.0);
    assert_eq!(result["per_sample"][0]["ari"], 1.0);

    let missing = mosna_mcp::compare::compare_partitions(
        (a.path(), &naming, "niches_9"),
        (b.path(), &naming, "niches"),
        false,
    )
    .unwrap_err();
    assert!(missing.to_string().contains("niches_1"), "{missing}");
}
