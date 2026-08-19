//! Tests of the command line interface.
//!
//! Written before the implementation. The command surface is part of the
//! "identical usage" promise: the GUI launches these as sub-processes, so the
//! sub-command names and the `--file` / `--working_dir` flags are a contract,
//! not an implementation detail.

use std::path::{Path, PathBuf};

use mosna_cli::{run, Cli, Command};
use mosna_io::Table;

/// A working directory with one sample, plus a configuration file on disk.
struct Fixture {
    _dir: tempfile::TempDir,
    working_dir: PathBuf,
    config_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let working_dir = dir.path().to_path_buf();
        let raw = working_dir.join("raw");
        std::fs::create_dir_all(&raw).unwrap();

        let side = 6;
        let n = side * side;
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let mut labels = Vec::new();
        for i in 0..n {
            xs.push((i / side) as f64 + ((i * 7) % 5) as f64 * 0.03);
            ys.push((i % side) as f64 + ((i * 11) % 5) as f64 * 0.03);
            labels.push(if i % 2 == 0 { "A" } else { "B" });
        }
        let table = Table::from_columns(vec![
            ("X_position".into(), Table::f64_array(xs)),
            ("Y_position".into(), Table::f64_array(ys)),
            ("Cluster".into(), Table::string_array(labels)),
        ])
        .unwrap();
        mosna_io::write::write_parquet::write_parquet(
            &table,
            raw.join("nodes_patient-1_sample-1.parquet"),
        )
        .unwrap();

        let config_path = working_dir.join("configuration.yaml");
        std::fs::write(&config_path, CONFIG).unwrap();

        Self {
            _dir: dir,
            working_dir,
            config_path,
        }
    }

    fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    fn config(&self) -> &Path {
        &self.config_path
    }
}

const CONFIG: &str = "\
Tysserand:
  Nodes directory: raw
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
  X coordinates column: X_position
  Y coordinates column: Y_position
  Phenotype column: Cluster
  Edges method: delaunay
  Min neighbors: 3
  CPU: 2
Assortativity:
  Network directory: Default
  Phenotype column: Cluster
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
  Index: index
  Number of shuffle: 10
  Randomization diagnostic: false
Niche Analysis:
  Network directory: Default
  Saving directory: niche_cluster
  Extension: parquet
  Patient column name: patient
  Sample column name: sample
  Processing method: Aggregated nodes
  Niches method: NAS
  Phenotype column: Cluster
  Column to aggregate: Cluster
  Plot Network: false
  X coordinates column for niches: X_position
  Y coordinates column for niches: Y_position
  CPU: 2
  Aggregated nodes:
    reducer_type: umap
    dim_clust: 2
    n_neighbors: 8
    metric: euclidean
    min_dist: 0.0
    clusterer_type: gmm
    k_cluster: 8
    n_clusters: 2
    resolution: 0.05
    min_cluster_size: 5
    normalize: total
    order: '1'
    stat_funcs: np.mean,np.std
    stat_names: [mean, std]
  Per sample:
    reducer_type: umap
    dim_clust: 2
    n_neighbors: 8
    metric: euclidean
    min_dist: 0.0
    clusterer_type: gmm
    k_cluster: 8
    n_clusters: 2
    resolution: 0.05
    min_cluster_size: 5
    normalize: total
    order: '1'
    stat_funcs: np.mean,np.std
    stat_names: [mean, std]
";

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

/// The sub-command names mirror the Python module names, with the underscores
/// the CLI convention replaces by hyphens. The GUI builds these command lines,
/// so a rename here silently breaks every button.
#[test]
fn the_sub_commands_are_named_after_the_python_modules() {
    for (argv, expected) in [
        (
            vec![
                "mosna",
                "tysserand-network",
                "--file",
                "c.yaml",
                "--working_dir",
                "/w",
            ],
            "tysserand-network",
        ),
        (
            vec![
                "mosna",
                "assortativity",
                "--file",
                "c.yaml",
                "--working_dir",
                "/w",
            ],
            "assortativity",
        ),
        (
            vec![
                "mosna",
                "niche-analysis",
                "--file",
                "c.yaml",
                "--working_dir",
                "/w",
            ],
            "niche-analysis",
        ),
        (
            vec!["mosna", "clear-temporary", "--working_dir", "/w"],
            "clear-temporary",
        ),
    ] {
        let cli =
            Cli::parse_from(&argv).unwrap_or_else(|e| panic!("`{expected}` did not parse: {e}"));
        assert_eq!(cli.command.name(), expected);
    }
}

#[test]
fn the_flags_match_the_python_argument_parser() {
    let cli = Cli::parse_from([
        "mosna",
        "tysserand-network",
        "--file",
        "/etc/mosna/configuration.yaml",
        "--working_dir",
        "/data/run",
    ])
    .unwrap();

    match cli.command {
        Command::TysserandNetwork { file, working_dir } => {
            assert_eq!(file, Path::new("/etc/mosna/configuration.yaml"));
            assert_eq!(working_dir, Path::new("/data/run"));
        }
        other => panic!("parsed the wrong command: {other:?}"),
    }
}

#[test]
fn the_configuration_flag_is_mandatory_for_the_analyses() {
    assert!(
        Cli::parse_from(["mosna", "tysserand-network", "--working_dir", "/w"]).is_err(),
        "--file must be required"
    );
    assert!(
        Cli::parse_from(["mosna", "assortativity", "--file", "c.yaml"]).is_err(),
        "--working_dir must be required"
    );
}

/// Clearing the temporary files needs no configuration, exactly like
/// `clear_temporary.py`, which only declares `--working_dir`.
#[test]
fn clearing_needs_only_the_working_directory() {
    let cli = Cli::parse_from(["mosna", "clear-temporary", "--working_dir", "/w"]).unwrap();
    match cli.command {
        Command::ClearTemporary { working_dir } => assert_eq!(working_dir, Path::new("/w")),
        other => panic!("parsed the wrong command: {other:?}"),
    }
}

#[test]
fn an_unknown_sub_command_is_rejected() {
    assert!(Cli::parse_from(["mosna", "not-a-step", "--working_dir", "/w"]).is_err());
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

#[test]
fn the_three_analyses_run_in_sequence() {
    let fixture = Fixture::new();

    run(Cli::parse_from([
        "mosna",
        "tysserand-network",
        "--file",
        fixture.config().to_str().unwrap(),
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap();
    assert!(fixture
        .working_dir()
        .join("temp/net_dir_mosna/nodes_patient-1_sample-1.parquet")
        .is_file());

    run(Cli::parse_from([
        "mosna",
        "assortativity",
        "--file",
        fixture.config().to_str().unwrap(),
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap();
    assert!(fixture
        .working_dir()
        .join("Assortativity/net_stat.csv")
        .is_file());

    run(Cli::parse_from([
        "mosna",
        "niche-analysis",
        "--file",
        fixture.config().to_str().unwrap(),
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap();
    assert!(fixture
        .working_dir()
        .join("Niche_Analysis/Aggregation/niche_cluster/parameters.json")
        .is_file());
}

#[test]
fn clearing_removes_the_temporary_directory() {
    let fixture = Fixture::new();
    run(Cli::parse_from([
        "mosna",
        "tysserand-network",
        "--file",
        fixture.config().to_str().unwrap(),
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap();
    assert!(fixture.working_dir().join("temp").is_dir());

    run(Cli::parse_from([
        "mosna",
        "clear-temporary",
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap();
    assert!(!fixture.working_dir().join("temp").exists());
}

/// A missing configuration file must be reported by path, not as a panic or a
/// bare "No such file" the GUI cannot explain to the user.
#[test]
fn a_missing_configuration_is_reported_by_path() {
    let fixture = Fixture::new();
    let error = run(Cli::parse_from([
        "mosna",
        "assortativity",
        "--file",
        "/nonexistent/configuration.yaml",
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("/nonexistent/configuration.yaml"),
        "{message}"
    );
}

/// A configuration the validator rejects must surface the validator's own
/// message, which is what the GUI shows in its error dialog.
#[test]
fn an_invalid_configuration_surfaces_the_validation_message() {
    let fixture = Fixture::new();
    let broken = fixture.working_dir().join("broken.yaml");
    std::fs::write(&broken, CONFIG.replace("CPU: 2", "CPU: plenty")).unwrap();

    let error = run(Cli::parse_from([
        "mosna",
        "tysserand-network",
        "--file",
        broken.to_str().unwrap(),
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap_err();

    assert!(
        error.to_string().contains("CPU parameter must be int"),
        "{error}"
    );
}

/// The report is the last thing a user does with a working directory, and it
/// has to describe what the analyses actually left there — the real files, drawn
/// by the real renderer, not a fixture.
#[test]
fn the_report_describes_what_the_analyses_wrote() {
    let fixture = Fixture::new();

    for step in ["tysserand-network", "assortativity"] {
        run(Cli::parse_from([
            "mosna",
            step,
            "--file",
            fixture.config().to_str().unwrap(),
            "--working_dir",
            fixture.working_dir().to_str().unwrap(),
        ])
        .unwrap())
        .unwrap();
    }

    run(Cli::parse_from([
        "mosna",
        "generate-report",
        "--working_dir",
        fixture.working_dir().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap();

    let report = fixture.working_dir().join("report.html");
    assert!(report.is_file(), "no report was written");
    let page = std::fs::read_to_string(&report).unwrap();

    assert!(page.starts_with("<!doctype html>"));
    // The figures of step 2, under the names the analyses wrote them with.
    assert!(
        page.contains("abundance"),
        "the abundance figure is missing"
    );
    assert!(
        page.contains("Assortativity/abundance.html"),
        "the interactive chart is not linked"
    );
    // And the table beside them, which is not a figure but is in the directory.
    assert!(page.contains("net_stat.csv"), "the listing is incomplete");
}

/// Generating a report on a directory nothing has been run in is the first
/// thing a new user will do by accident. It must produce a page, not an error.
#[test]
fn a_report_on_an_empty_directory_is_not_a_failure() {
    let dir = tempfile::tempdir().unwrap();

    run(Cli::parse_from([
        "mosna",
        "generate-report",
        "--working_dir",
        dir.path().to_str().unwrap(),
    ])
    .unwrap())
    .unwrap();

    let page = std::fs::read_to_string(dir.path().join("report.html")).unwrap();
    assert!(page.to_lowercase().contains("no figure"));
}
