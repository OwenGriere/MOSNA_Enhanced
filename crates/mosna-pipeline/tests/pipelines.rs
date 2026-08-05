//! End-to-end tests of the four analyses.
//!
//! Written before the implementations. These assert the *contract with the
//! filesystem*: which files each step produces, where, and with what content.
//! That contract is what the GUI reads back and what the next step consumes, so
//! it is the part that must not drift from the Python.

use std::path::{Path, PathBuf};

use mosna_config::RawConfig;
use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::{find_sample, Table};
use mosna_pipeline::{
    assortativity, clear_temporary, niche_analysis, tysserand_network, NoFigures, SilentProgress,
};

/// A working directory holding a `raw` sub-directory of nodes files.
struct Workspace {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Workspace {
    /// Three samples of `n_cells` cells each, laid out on a jittered grid so
    /// the triangulation is non-degenerate.
    fn new(n_samples: usize, n_cells: usize, phenotypes: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let raw = root.join("raw");
        std::fs::create_dir_all(&raw).unwrap();

        for sample in 1..=n_samples {
            let mut xs = Vec::with_capacity(n_cells);
            let mut ys = Vec::with_capacity(n_cells);
            let mut labels = Vec::with_capacity(n_cells);
            let side = (n_cells as f64).sqrt().ceil() as usize;
            for i in 0..n_cells {
                let (row, column) = (i / side, i % side);
                // The jitter keeps the points off a perfect lattice, where the
                // triangulation would be ambiguous.
                xs.push(row as f64 + ((i * 7) % 5) as f64 * 0.03);
                ys.push(column as f64 + ((i * 11) % 5) as f64 * 0.03);
                labels.push(phenotypes[i % phenotypes.len()]);
            }

            let table = Table::from_columns(vec![
                ("X_position".into(), Table::f64_array(xs)),
                ("Y_position".into(), Table::f64_array(ys)),
                ("Cluster".into(), Table::string_array(labels)),
            ])
            .unwrap();
            mosna_io::write::write_parquet::write_parquet(
                &table,
                raw.join(format!("nodes_patient-{sample}_sample-1.parquet")),
            )
            .unwrap();
        }

        Self { _dir: dir, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn net_dir(&self) -> PathBuf {
        self.root.join("temp/net_dir_mosna")
    }
}

/// Counts the figures a run asks for, so a test can assert that a figure which
/// would be meaningless was not drawn.
#[derive(Default)]
struct CountingFigures {
    embeddings: std::sync::atomic::AtomicUsize,
}

impl CountingFigures {
    fn embeddings(&self) -> usize {
        self.embeddings.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl mosna_pipeline::FigureSink for CountingFigures {
    fn embedding(
        &self,
        _embedding: &[f64],
        _n_components: usize,
        _labels: &[u32],
        _save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        self.embeddings
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// A configuration covering all three analyses, pointing at `raw`.
fn config(saving_directory: &str) -> RawConfig {
    config_with_reducer(saving_directory, "umap")
}

/// The same, with the dimensionality reduction of both niche sub-sections set
/// to `reducer` — `none` sends the features straight to the clusterer.
fn config_with_reducer(saving_directory: &str, reducer: &str) -> RawConfig {
    let yaml = format!(
        "\
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
  CPU: 4
Assortativity:
  Network directory: Default
  Phenotype column: Cluster
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
  Index: index
  Number of shuffle: 20
  Randomization diagnostic: false
Niche Analysis:
  Network directory: Default
  Saving directory: {saving_directory}
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
  CPU: 4
  Aggregated nodes:
    reducer_type: {reducer}
    dim_clust: 2
    n_neighbors: 10
    metric: euclidean
    min_dist: 0.0
    clusterer_type: gmm
    k_cluster: 10
    n_clusters: 3
    resolution: 0.05
    min_cluster_size: 10
    normalize: total
    order: '1'
    stat_funcs: np.mean,np.std
    stat_names: [mean, std]
  Per sample:
    reducer_type: {reducer}
    dim_clust: 2
    n_neighbors: 10
    metric: euclidean
    min_dist: 0.0
    clusterer_type: gmm
    k_cluster: 10
    n_clusters: 3
    resolution: 0.05
    min_cluster_size: 10
    normalize: total
    order: '1'
    stat_funcs: np.mean,np.std
    stat_names: [mean, std]
"
    );
    RawConfig::from_yaml_str(&yaml).unwrap()
}

// ---------------------------------------------------------------------------
// Step 1 — Tysserand
// ---------------------------------------------------------------------------

/// Step 1 must write a nodes and an edges file per sample into
/// `temp/net_dir_mosna`, which is where steps 2 and 3 look by default.
#[test]
fn tysserand_writes_a_network_per_sample() {
    let workspace = Workspace::new(3, 36, &["A", "B"]);
    tysserand_network(
        &config("niche_cluster"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let nodes = find_sample(workspace.net_dir(), "parquet", "patient", Some("sample")).unwrap();
    assert_eq!(nodes.len(), 3, "one nodes file per sample");

    for path in &nodes {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let edges_path = workspace
            .net_dir()
            .join(name.replacen("nodes_", "edges_", 1));
        assert!(edges_path.is_file(), "{edges_path:?} is missing");

        let nodes_table = read_table(path, Extension::Parquet).unwrap();
        assert_eq!(nodes_table.n_rows(), 36, "every cell must be kept");
        assert!(nodes_table.has_column("Cluster"));

        let edges_table = read_table(&edges_path, Extension::Parquet).unwrap();
        let pairs = edges_table.edges().unwrap();
        assert!(!pairs.is_empty(), "the network has no edges");
        assert!(
            pairs
                .iter()
                .all(|&(a, b)| (a as usize) < 36 && (b as usize) < 36),
            "an edge points outside the sample"
        );
    }
}

/// The reconstructed network must connect every cell: an isolated cell has no
/// neighbourhood, so its niche feature vector would be its own attributes alone.
#[test]
fn tysserand_leaves_no_cell_isolated() {
    let workspace = Workspace::new(1, 36, &["A"]);
    tysserand_network(
        &config("niche_cluster"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let edges = read_table(
        workspace.net_dir().join("edges_patient-1_sample-1.parquet"),
        Extension::Parquet,
    )
    .unwrap();
    let mut degree = vec![0usize; 36];
    for (a, b) in edges.edges().unwrap() {
        degree[a as usize] += 1;
        degree[b as usize] += 1;
    }
    assert!(
        degree.iter().all(|&d| d > 0),
        "a cell has no edge: {degree:?}"
    );
}

#[test]
fn tysserand_is_reproducible() {
    let workspace = Workspace::new(1, 25, &["A", "B"]);
    let mut runs = Vec::new();
    for _ in 0..2 {
        tysserand_network(
            &config("niche_cluster"),
            workspace.root(),
            &SilentProgress,
            &NoFigures,
        )
        .unwrap();
        let edges = read_table(
            workspace.net_dir().join("edges_patient-1_sample-1.parquet"),
            Extension::Parquet,
        )
        .unwrap();
        runs.push(edges.edges().unwrap());
    }
    assert_eq!(runs[0], runs[1]);
}

#[test]
fn tysserand_reports_a_missing_column() {
    let workspace = Workspace::new(1, 16, &["A"]);
    let mut broken = config("niche_cluster");
    broken.set(
        "Tysserand",
        "Phenotype column",
        serde_yaml::Value::String("NotThere".into()),
    );

    let err =
        tysserand_network(&broken, workspace.root(), &SilentProgress, &NoFigures).unwrap_err();
    assert!(err.to_string().contains("NotThere"), "{err}");
}

// ---------------------------------------------------------------------------
// Step 2 — Assortativity
// ---------------------------------------------------------------------------

/// Step 2 writes `Assortativity/net_stat.csv` with one row per sample and the
/// column layout the figures and the GUI expect.
#[test]
fn assortativity_writes_the_statistics_table() {
    let workspace = Workspace::new(3, 36, &["A", "B"]);
    let configuration = config("niche_cluster");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();
    assortativity(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let path = workspace.root().join("Assortativity/net_stat.csv");
    assert!(path.is_file(), "net_stat.csv was not written");

    let table = read_table(&path, Extension::Csv).unwrap();
    assert_eq!(table.n_rows(), 3, "one row per sample");

    let names = table.column_names();
    assert_eq!(names[0], "id", "the index column must come first");
    // `B - A`, not `A - B`: the reference names the elements of the lower
    // triangle, larger index first, and the values are flattened in that same
    // order. This test used to expect `A - B`, which is how the mismatch
    // between the names and the values survived — it pinned the wrong shape.
    for expected in ["# total", "% A", "% B", "assort", "assort Z", "B - A Z"] {
        assert!(names.contains(&expected), "missing column `{expected}`");
    }
    assert!(
        !names.contains(&"A - B Z"),
        "the upper-triangle spelling is the one that mislabelled the values"
    );

    // The ids must name the samples the way the rest of the pipeline does.
    let ids = table.string_column("id").unwrap();
    assert!(ids.contains(&"patient-1_sample-1".to_string()), "{ids:?}");

    // Every sample holds all its cells.
    for total in table.f64_column("# total").unwrap() {
        assert_eq!(total, 36.0);
    }
}

#[test]
fn assortativity_is_reproducible() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config("niche_cluster");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let mut runs = Vec::new();
    for _ in 0..2 {
        assortativity(
            &configuration,
            workspace.root(),
            &SilentProgress,
            &NoFigures,
        )
        .unwrap();
        let table = read_table(
            workspace.root().join("Assortativity/net_stat.csv"),
            Extension::Csv,
        )
        .unwrap();
        runs.push(table.f64_column("assort").unwrap());
    }
    assert_eq!(runs[0], runs[1]);
}

/// The diagnostic mode is a timing probe: it shuffles a fixed twenty times and
/// writes nothing, so the GUI can extrapolate the cost of the real run.
#[test]
fn the_randomization_diagnostic_writes_nothing() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let mut configuration = config("niche_cluster");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    configuration.set(
        "Assortativity",
        "Randomization diagnostic",
        serde_yaml::Value::Bool(true),
    );
    assortativity(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    assert!(
        !workspace
            .root()
            .join("Assortativity/net_stat.csv")
            .is_file(),
        "the diagnostic run must not write results"
    );
}

// ---------------------------------------------------------------------------
// Step 3 — Niche analysis
// ---------------------------------------------------------------------------

/// Step 3 writes its results under `Niche_Analysis/Aggregation/<saving dir>`,
/// records the parameters it ran with, and writes the niche label of every cell
/// back into the network files.
#[test]
fn niche_analysis_writes_results_and_labels_the_cells() {
    let workspace = Workspace::new(3, 36, &["A", "B", "C"]);
    let configuration = config("run one");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();
    niche_analysis(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let save_dir = workspace.root().join("Niche_Analysis/Aggregation/run one");
    assert!(save_dir.is_dir(), "{save_dir:?} was not created");
    assert!(
        save_dir.join("parameters.json").is_file(),
        "the run parameters were not recorded"
    );

    // Every cell carries a niche label.
    let nodes = find_sample(workspace.net_dir(), "parquet", "patient", Some("sample")).unwrap();
    assert_eq!(nodes.len(), 3);
    for path in &nodes {
        let table = read_table(path, Extension::Parquet).unwrap();
        assert!(table.has_column("niches"), "{path:?} has no niches column");
        let niches = table.f64_column("niches").unwrap();
        assert_eq!(niches.len(), 36);
        assert!(niches.iter().all(|v| v.is_finite() && *v >= 0.0));
    }
}

/// The reduction is optional. With `reducer_type: none` the run goes straight
/// from the aggregated features to the clustering, and still labels every cell.
#[test]
fn niche_analysis_runs_without_a_reduction() {
    let workspace = Workspace::new(3, 36, &["A", "B", "C"]);
    let configuration = config_with_reducer("no reduction", "none");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();
    niche_analysis(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let save_dir = workspace
        .root()
        .join("Niche_Analysis/Aggregation/no reduction");
    assert!(save_dir.is_dir(), "{save_dir:?} was not created");
    assert!(save_dir.join("parameters.json").is_file());

    let nodes = find_sample(workspace.net_dir(), "parquet", "patient", Some("sample")).unwrap();
    for path in &nodes {
        let table = read_table(path, Extension::Parquet).unwrap();
        assert!(table.has_column("niches"), "{path:?} has no niches column");
        let niches = table.f64_column("niches").unwrap();
        assert_eq!(niches.len(), 36);
        assert!(niches.iter().all(|v| v.is_finite() && *v >= 0.0));
    }
}

/// The scatter of the clusters is a picture of the *projection*. Without one
/// there is no plane to draw it in — the first two feature columns are two
/// phenotypes, not two axes — so the figure is skipped rather than faked.
#[test]
fn the_cluster_scatter_is_drawn_only_when_there_is_a_projection() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);

    let reduced = CountingFigures::default();
    let configuration = config_with_reducer("reduced", "umap");
    tysserand_network(&configuration, workspace.root(), &SilentProgress, &reduced).unwrap();
    niche_analysis(&configuration, workspace.root(), &SilentProgress, &reduced).unwrap();
    assert_eq!(reduced.embeddings(), 1, "the projection was not drawn");

    let unreduced = CountingFigures::default();
    let configuration = config_with_reducer("unreduced", "none");
    niche_analysis(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &unreduced,
    )
    .unwrap();
    assert_eq!(
        unreduced.embeddings(),
        0,
        "there is no projection to scatter"
    );
}

/// The aggregated features are cached so a re-run does not recompute them,
/// which is what `if (temp_dir / 'var_aggreg.parquet').exists()` does in the
/// Python.
#[test]
fn niche_analysis_caches_the_feature_table() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config("cached");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();
    niche_analysis(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let cache = workspace.net_dir().join("var_aggreg.parquet");
    assert!(cache.is_file(), "the feature table was not cached");

    let table = read_table(&cache, Extension::Parquet).unwrap();
    assert_eq!(table.n_rows(), 50, "two samples of twenty-five cells");
    // Two statistics per phenotype, plus the two identifier columns.
    assert!(table.has_column("A mean"));
    assert!(table.has_column("A std"));
    assert!(table.has_column("patient"));
}

#[test]
fn niche_analysis_is_reproducible() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config("repeat");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let mut runs = Vec::new();
    for _ in 0..2 {
        niche_analysis(
            &configuration,
            workspace.root(),
            &SilentProgress,
            &NoFigures,
        )
        .unwrap();
        let table = read_table(
            workspace.net_dir().join("nodes_patient-1_sample-1.parquet"),
            Extension::Parquet,
        )
        .unwrap();
        runs.push(table.f64_column("niches").unwrap());
    }
    assert_eq!(runs[0], runs[1]);
}

#[test]
fn niche_analysis_rejects_an_invalid_saving_directory() {
    let workspace = Workspace::new(1, 16, &["A"]);
    let mut broken = config("../escape");
    tysserand_network(&broken, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    broken.set(
        "Niche Analysis",
        "Saving directory",
        serde_yaml::Value::String("../escape".into()),
    );

    let err = niche_analysis(&broken, workspace.root(), &SilentProgress, &NoFigures).unwrap_err();
    assert!(err.to_string().contains("not valid"), "{err}");
}

// ---------------------------------------------------------------------------
// Clear temporary files
// ---------------------------------------------------------------------------

#[test]
fn clearing_removes_the_temporary_directory() {
    let workspace = Workspace::new(1, 16, &["A"]);
    tysserand_network(
        &config("niche_cluster"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();
    assert!(workspace.net_dir().is_dir());

    clear_temporary(workspace.root(), &SilentProgress).unwrap();
    assert!(!workspace.root().join("temp").exists());
}

#[test]
fn clearing_an_absent_directory_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    clear_temporary(dir.path(), &SilentProgress).unwrap();
}
