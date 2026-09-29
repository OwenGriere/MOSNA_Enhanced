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
    assortativity, clear_temporary, niche_analysis, niche_analysis_in, tysserand_network,
    NoFigures, SilentProgress,
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
    networks: std::sync::atomic::AtomicUsize,
}

impl CountingFigures {
    fn embeddings(&self) -> usize {
        self.embeddings.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn networks(&self) -> usize {
        self.networks.load(std::sync::atomic::Ordering::Relaxed)
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

    fn network(
        &self,
        _sample: &mosna_io::SampleId,
        _patient_column: &str,
        _sample_column: Option<&str>,
        _coords: &[[f64; 2]],
        _pairs: &[(u32, u32)],
        _labels: &[String],
        _save_dir: &Path,
    ) -> mosna_pipeline::Result<()> {
        self.networks
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// A configuration covering all three analyses, pointing at `raw`.
fn config() -> RawConfig {
    config_with_reducer("umap")
}

/// The same, with the dimensionality reduction of both niche sub-sections set
/// to `reducer` — `none` sends the features straight to the clusterer.
fn config_with_reducer(reducer: &str) -> RawConfig {
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
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

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
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

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
        tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
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
    let mut broken = config();
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
    let configuration = config();
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
    let configuration = config();
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
    let mut configuration = config();
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

/// Step 3 writes its results under `Niche_Analysis/<run number>`, records the
/// parameters it ran with, and writes the niche label of every cell back into
/// the network files.
#[test]
fn niche_analysis_writes_results_and_labels_the_cells() {
    let workspace = Workspace::new(3, 36, &["A", "B", "C"]);
    let configuration = config();
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

    let save_dir = workspace.root().join("Niche_Analysis/1-1-1");
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
        // The column is named after the run, so a later run adds a second one
        // rather than replacing these labels.
        assert!(
            table.has_column("niches_1-1-1"),
            "{path:?} has no niches_1-1-1 column"
        );
        let niches = table.f64_column("niches_1-1-1").unwrap();
        assert_eq!(niches.len(), 36);
        assert!(niches.iter().all(|v| v.is_finite() && *v >= 0.0));
    }
}

/// The reduction is optional. With `reducer_type: none` the run goes straight
/// from the aggregated features to the clustering, and still labels every cell.
#[test]
fn niche_analysis_runs_without_a_reduction() {
    let workspace = Workspace::new(3, 36, &["A", "B", "C"]);
    let configuration = config_with_reducer("none");
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

    let save_dir = workspace.root().join("Niche_Analysis/1-0-1");
    assert!(save_dir.is_dir(), "{save_dir:?} was not created");
    assert!(save_dir.join("parameters.json").is_file());

    let nodes = find_sample(workspace.net_dir(), "parquet", "patient", Some("sample")).unwrap();
    for path in &nodes {
        let table = read_table(path, Extension::Parquet).unwrap();
        assert!(
            table.has_column("niches_1-0-1"),
            "{path:?} has no niches_1-0-1 column"
        );
        let niches = table.f64_column("niches_1-0-1").unwrap();
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
    let configuration = config_with_reducer("umap");
    tysserand_network(&configuration, workspace.root(), &SilentProgress, &reduced).unwrap();
    niche_analysis(&configuration, workspace.root(), &SilentProgress, &reduced).unwrap();
    assert_eq!(reduced.embeddings(), 1, "the projection was not drawn");

    let unreduced = CountingFigures::default();
    let configuration = config_with_reducer("none");
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

/// Overwrite a cached partition, keeping the source it records.
///
/// The cache refuses a file that does not say what it was computed from, so a
/// test that plants an answer has to plant a well-formed one — otherwise it
/// would be testing the provenance check instead of the reuse.
fn plant(partition: &Path, labels: &[u32]) {
    let source = mosna_io::read::read_parquet::read_parquet_key(partition, "mosna.source")
        .unwrap()
        .expect("the cached partition records no source");
    mosna_pipeline::niche_cache::write_labels(partition, &source, labels).unwrap();
}

/// The numbered run directories under `Niche_Analysis`, sorted.
///
/// The register and the lock that guards it sit in the same directory and are
/// not runs, so a test counting runs must not count them.
fn run_directories(niche_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(niche_dir)
        .unwrap_or_else(|e| panic!("{niche_dir:?}: {e}"))
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// The files in a cache directory, sorted.
fn names_in(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap_or_else(|e| panic!("{directory:?}: {e}"))
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// The run register, parsed.
fn register_of(root: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(root.join("Niche_Analysis/runs.json")).unwrap())
        .unwrap()
}

/// Records what the pipeline says, so a test can assert on a warning.
#[derive(Default)]
struct Spoken(std::sync::Mutex<Vec<String>>);

impl mosna_pipeline::progress::Progress for Spoken {
    fn info(&self, message: &str) {
        self.0.lock().unwrap().push(message.to_string());
    }
    fn step(&self, _current: usize, _total: usize, _description: &str) {}
}

impl Spoken {
    fn said(&self, fragment: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains(fragment))
    }

    /// Everything that was said, for an assertion message that shows why it
    /// failed rather than only that it did.
    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

/// The aggregated features are cached so a re-run does not recompute them.
///
/// The file is named after the settings that produced it, in a directory of its
/// own, so two configurations can each keep theirs.
#[test]
fn niche_analysis_caches_the_feature_table() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config();
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

    let cache = workspace
        .root()
        .join("temp/intermediate_files/var_aggreg-1/var_aggreg_1.parquet");
    assert!(
        cache.is_file(),
        "the feature table was not cached at {cache:?}"
    );

    let table = read_table(&cache, Extension::Parquet).unwrap();
    assert_eq!(table.n_rows(), 50, "two samples of twenty-five cells");
    // Two statistics per phenotype, plus the two identifier columns.
    assert!(table.has_column("A mean"));
    assert!(table.has_column("A std"));
    assert!(table.has_column("patient"));
}

/// The projection and the partition are cached too, side by side, told apart by
/// the prefix their names carry.
#[test]
fn niche_analysis_caches_the_projection_and_the_partition() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config();
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

    // The files nest the way the stages do: a partition beside its projection,
    // inside the aggregation that projection came from.
    assert_eq!(
        names_in(
            &workspace
                .root()
                .join("temp/intermediate_files/var_aggreg-1/reduction-1")
        ),
        vec!["clustering_1.parquet", "reduction_1.parquet"]
    );

    // The catalogue is three dictionaries, one per stage, each numbered.
    let catalogue = register_of(workspace.root());

    let features = &catalogue["1"];
    assert_eq!(features["mode"], "aggregated");
    assert_eq!(
        features["path"],
        "temp/intermediate_files/var_aggreg-1/var_aggreg_1.parquet"
    );
    assert_eq!(features["parameters"]["order"], 1);
    assert_eq!(features["parameters"]["column_to_aggregate"][0], "Cluster");
    assert_eq!(features["sample"], "");

    let reduction = &features["reduction"]["1"];
    assert_eq!(reduction["id"], 1);
    assert_eq!(
        reduction["path"],
        "temp/intermediate_files/var_aggreg-1/reduction-1/reduction_1.parquet"
    );
    // The settings are spelled out, not encoded into the name.
    assert_eq!(reduction["parameters"]["reducer_type"], "umap");
    assert_eq!(reduction["parameters"]["dim_clust"], 2);
    assert_eq!(reduction["parameters"]["metric"], "euclidean");
    assert!(reduction["updated"].is_string());

    let clustering = &reduction["clustering"]["1"];
    assert_eq!(clustering["parameters"]["clusterer_type"], "gmm");
    assert_eq!(clustering["parameters"]["n_clusters"], 3);
    // A clusterer that does not read `resolution` does not record it: two
    // identical GMM runs must not look different because it moved.
    assert!(clustering["parameters"]["resolution"].is_null());
    assert_eq!(
        clustering["path"],
        "temp/intermediate_files/var_aggreg-1/reduction-1/clustering_1.parquet"
    );
}

/// The whole point of the caches: a second run reads them instead of computing
/// again. Proved by making the cached partition say something the clusterer
/// never would, and finding it in the cells afterwards.
#[test]
fn a_second_run_reads_the_cached_partition() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config();
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

    let partition = workspace
        .root()
        .join("temp/intermediate_files/var_aggreg-1/reduction-1/clustering_1.parquet");

    // Every cell into niche 7 — an answer no clusterer would return here.
    // Written through the cache's own writer so it keeps the recorded source:
    // a partition that does not say what it came from is rejected, which is
    // the subject of its own test below.
    plant(&partition, &[7u32; 50]);

    let spoken = Spoken::default();
    niche_analysis(&configuration, workspace.root(), &spoken, &NoFigures).unwrap();
    assert!(
        spoken.said("Reusing the cached partition"),
        "the cache was not read"
    );

    let nodes = find_sample(workspace.net_dir(), "parquet", "patient", Some("sample")).unwrap();
    for path in &nodes {
        let table = read_table(path, Extension::Parquet).unwrap();
        let niches = table.f64_column("niches_1-1-1").unwrap();
        assert!(niches.iter().all(|v| *v == 7.0), "{path:?} was recomputed");
    }
}

/// A cache belongs to the cohort it was computed on. One that has the right
/// name and the wrong height is from another cohort, and must be recomputed.
#[test]
fn a_cache_of_the_wrong_height_is_not_trusted() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config();
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

    let partition = workspace
        .root()
        .join("temp/intermediate_files/var_aggreg-1/reduction-1/clustering_1.parquet");
    plant(&partition, &[7u32; 3]);

    let spoken = Spoken::default();
    niche_analysis(&configuration, workspace.root(), &spoken, &NoFigures).unwrap();
    assert!(!spoken.said("Reusing the cached partition"));

    let nodes = find_sample(workspace.net_dir(), "parquet", "patient", Some("sample")).unwrap();
    let table = read_table(&nodes[0], Extension::Parquet).unwrap();
    assert!(table
        .f64_column("niches_1-1-1")
        .unwrap()
        .iter()
        .any(|v| *v != 7.0));
}

/// Changing the aggregation alone must invalidate *both* downstream caches.
///
/// `order` does not appear in the projection's or the partition's file name —
/// each carries only its own settings — so both names are unchanged while the
/// matrices behind them are not. This is the case the recorded source exists
/// for, and the one that reused a stale partition until it was measured.
#[test]
fn changing_the_features_invalidates_the_projection_and_the_partition() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let first = config();
    tysserand_network(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    assert!(workspace
        .root()
        .join("temp/intermediate_files/var_aggreg-1/reduction-1/reduction_1.parquet")
        .is_file());

    // A different neighbourhood order: different features, same settings
    // everywhere downstream, and therefore the same two file names.
    let mut second = config();
    let mut aggregated = second
        .get("Niche Analysis", "Aggregated nodes")
        .unwrap()
        .clone();
    aggregated["order"] = serde_yaml::Value::String("2".into());
    second.set("Niche Analysis", "Aggregated nodes", aggregated);

    let spoken = Spoken::default();
    niche_analysis(&second, workspace.root(), &spoken, &NoFigures).unwrap();

    assert!(
        !spoken.said("Reusing the cached projection"),
        "the projection of the previous features was reused"
    );
    assert!(
        !spoken.said("Reusing the cached partition"),
        "the partition of the previous projection was reused"
    );

    // The second aggregation gets a directory of its own, and its projection
    // starts again at one inside it — which is why nothing could collide.
    assert_eq!(
        names_in(&workspace.root().join("temp/intermediate_files")),
        vec!["var_aggreg-1", "var_aggreg-2"]
    );
    assert_eq!(
        names_in(
            &workspace
                .root()
                .join("temp/intermediate_files/var_aggreg-2/reduction-1")
        ),
        vec!["clustering_1.parquet", "reduction_1.parquet"]
    );
}

/// Only the settings a stage reads count as its settings.
///
/// The fixture clusters with GMM, which never looks at `resolution`. Moving it
/// changes nothing about the result, so it must not open a second run or a
/// second cache file — otherwise every stray edit to an unused parameter would
/// throw the cache away.
#[test]
fn a_parameter_the_stage_ignores_does_not_make_a_new_run() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let first = config();
    tysserand_network(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let mut second = config();
    let mut aggregated = second
        .get("Niche Analysis", "Aggregated nodes")
        .unwrap()
        .clone();
    aggregated["resolution"] = serde_yaml::Value::Number(0.9.into());
    second.set("Niche Analysis", "Aggregated nodes", aggregated);

    let spoken = Spoken::default();
    niche_analysis(&second, workspace.root(), &spoken, &NoFigures).unwrap();

    assert!(
        spoken.said("same settings were run before as Niche_Analysis/1-1-1"),
        "GMM does not read resolution, so this is the same run"
    );
    assert!(spoken.said("Reusing the cached partition"));
    assert!(!workspace.root().join("Niche_Analysis/2-2-2").exists());
    assert_eq!(
        names_in(
            &workspace
                .root()
                .join("temp/intermediate_files/var_aggreg-1/reduction-1")
        ),
        vec!["clustering_1.parquet", "reduction_1.parquet"]
    );
}

/// Runs are numbered, and the register says what each number holds.
#[test]
fn runs_are_numbered_and_registered() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let first = config();
    tysserand_network(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // A different reduction is a different pipeline, so a different run.
    let second = config_with_reducer("none");
    niche_analysis(&second, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // The second run aggregates identically, so it keeps aggregation 1. It has
    // no reduction — the one permitted zero — and its partition is numbered
    // among that branch's own, which is empty, so it is number one as well.
    assert!(workspace.root().join("Niche_Analysis/1-1-1").is_dir());
    assert!(workspace.root().join("Niche_Analysis/1-0-1").is_dir());

    // One aggregation shared by both runs; only what hangs under it differs.
    let catalogue = register_of(workspace.root());
    assert_eq!(catalogue.as_object().unwrap().len(), 1);
    assert_eq!(catalogue["1"]["mode"], "aggregated");

    let reductions = catalogue["1"]["reduction"].as_object().unwrap();
    assert_eq!(
        reductions.len(),
        2,
        "the reduced branch and the unreduced one"
    );
    assert_eq!(reductions["1"]["parameters"]["reducer_type"], "umap");
    assert_eq!(reductions["0"]["parameters"]["reducer_type"], "none");
    // Nothing to name when there is no projection.
    assert_eq!(reductions["0"]["path"], "");
    assert_eq!(
        reductions["0"]["clustering"]["1"]["path"],
        "temp/intermediate_files/var_aggreg-1/reduction-0/clustering_1.parquet"
    );
}

/// Re-running the same settings says so, names the directory it is writing
/// again, and does not open a third one.
#[test]
fn identical_settings_reuse_their_run_directory_with_a_warning() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config();
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

    let spoken = Spoken::default();
    niche_analysis(&configuration, workspace.root(), &spoken, &NoFigures).unwrap();

    assert!(
        spoken.said("same settings were run before as Niche_Analysis/1-1-1"),
        "the repeat was not announced, or did not name the directory"
    );
    assert!(!workspace.root().join("Niche_Analysis/2-2-2").exists());

    let catalogue = register_of(workspace.root());
    assert_eq!(catalogue.as_object().unwrap().len(), 1);
    let reductions = catalogue["1"]["reduction"].as_object().unwrap();
    assert_eq!(reductions.len(), 1, "a projection was added");
    assert_eq!(
        reductions["1"]["clustering"].as_object().unwrap().len(),
        1,
        "a partition was added"
    );
}

#[test]
fn niche_analysis_is_reproducible() {
    let workspace = Workspace::new(2, 25, &["A", "B"]);
    let configuration = config();
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
        runs.push(table.f64_column("niches_1-1-1").unwrap());
    }
    assert_eq!(runs[0], runs[1]);
}

// ---------------------------------------------------------------------------
// Clear temporary files
// ---------------------------------------------------------------------------

#[test]
fn clearing_removes_the_temporary_directory() {
    let workspace = Workspace::new(1, 16, &["A"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    assert!(workspace.net_dir().is_dir());

    clear_temporary(workspace.root(), &SilentProgress).unwrap();
    assert!(!workspace.root().join("temp").exists());
}

#[test]
fn clearing_an_absent_directory_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    clear_temporary(dir.path(), &SilentProgress).unwrap();
}

// ---------------------------------------------------------------------------
// The cache belongs to a cohort, not only to a set of settings
// ---------------------------------------------------------------------------

/// Rebuilding the network with a different edge rule changes every
/// neighbourhood, and therefore every aggregated feature vector — under niche
/// settings that have not moved at all.
///
/// The cache used to be keyed on the settings and the row count alone, so this
/// read back the features of the network that had just been replaced and wrote
/// out niches describing a graph that no longer existed. Nothing said so: the
/// labels came back bit-for-bit identical.
///
/// A cohort that changed is now a different aggregation, so it is numbered
/// afresh: the new results land in `2-1-1` rather than overwriting `1-1-1`,
/// which is what keeps two networks comparable instead of leaving one of them
/// silently replaced.
#[test]
fn rebuilding_the_network_invalidates_the_cached_features() {
    let workspace = Workspace::new(2, 36, &["A", "B", "C"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // A different network over the very same cells: same files, same rows,
    // same columns to aggregate, different edges.
    let knn = with_edges_method(config(), "knn");
    tysserand_network(&knn, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis(&knn, workspace.root(), &spoken, &NoFigures).unwrap();

    assert!(
        !spoken.said("Reusing the cached aggregated features"),
        "the features of the previous network were reused"
    );

    // A second aggregation, not a re-run of the first.
    let register = register_of(workspace.root());
    assert!(
        register.get("2").is_some(),
        "the rebuilt network did not get an aggregation of its own: {register}"
    );
    assert_ne!(
        register["1"]["parameters"]["cohort"], register["2"]["parameters"]["cohort"],
        "two different networks were recorded as the same cohort"
    );
    assert_eq!(
        register["1"]["parameters"]["order"], register["2"]["parameters"]["order"],
        "the settings were supposed to be identical"
    );

    // And its labels are written under their own column.
    let labels = labels_of(&workspace, "niches_2-1-1");
    assert_eq!(labels.len(), 72);
    assert!(labels.iter().all(|v| v.is_finite() && *v >= 0.0));
}

/// And the projection and the partition go with it: they are computed from the
/// features, so a stale feature table cannot leave a fresh embedding behind.
#[test]
fn rebuilding_the_network_invalidates_every_stage() {
    let workspace = Workspace::new(2, 36, &["A", "B", "C"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let knn = with_edges_method(config(), "knn");
    tysserand_network(&knn, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis(&knn, workspace.root(), &spoken, &NoFigures).unwrap();

    for stage in ["projection", "partition"] {
        assert!(
            !spoken.said(&format!("Reusing the cached {stage}")),
            "the {stage} of the previous network was reused"
        );
    }
}

/// A cohort that is left alone still gets its cache. The fingerprint has to
/// separate networks that differ, not defeat the reuse the cache exists for.
#[test]
fn an_untouched_cohort_still_reuses_every_stage() {
    let workspace = Workspace::new(2, 36, &["A", "B", "C"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis(&config(), workspace.root(), &spoken, &NoFigures).unwrap();

    for stage in ["aggregated features", "projection", "partition"] {
        assert!(
            spoken.said(&format!("Reusing the cached {stage}")),
            "the {stage} was recomputed although nothing changed"
        );
    }
}

/// Writing the labels back into the nodes files must not make the run look
/// like a cohort that changed: the fingerprint would then never match twice,
/// and the cache would be dead weight.
#[test]
fn the_labels_written_back_do_not_invalidate_the_cache() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // Three runs: the first fills the cache, the second writes a second label
    // column into every nodes file, the third must still hit the cache.
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    let other = with_n_clusters(config(), 2);
    niche_analysis(&other, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis(&config(), workspace.root(), &spoken, &NoFigures).unwrap();
    assert!(
        spoken.said("Reusing the cached aggregated features"),
        "writing a label column invalidated the aggregation it came from"
    );
}

// ---------------------------------------------------------------------------
// Writing the labels back is only possible into parquet
// ---------------------------------------------------------------------------

/// A custom network directory may be given any extension, and step 3 writes
/// the niche labels back into the nodes files it finds there. It can only
/// write parquet — so pointing it at a CSV directory used to overwrite every
/// `nodes_*.csv` with parquet bytes, destroying data the user supplied.
#[test]
fn a_network_directory_that_is_not_parquet_is_refused_before_anything_runs() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);

    // A CSV network directory, laid out the way the pipeline expects.
    let csv_dir = workspace.root().join("csv_net");
    std::fs::create_dir_all(&csv_dir).unwrap();
    let before = write_csv_cohort(&csv_dir);

    let configuration = with_network_directory(config(), "csv_net", "csv");
    let error = niche_analysis(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .expect_err("a CSV network directory was accepted");

    let message = error.to_string();
    assert!(message.contains("parquet"), "{message}");

    // And nothing was touched on the way to the refusal.
    for (path, contents) in before {
        assert_eq!(
            std::fs::read(&path).unwrap(),
            contents,
            "{path:?} was modified"
        );
    }
}

// ---------------------------------------------------------------------------
// What the run identity has to contain
// ---------------------------------------------------------------------------

/// Two runs that read different columns as their patient grouping split the
/// cohort differently, so their feature tables describe different things —
/// even when they happen to have the same height.
#[test]
fn the_grouping_columns_are_part_of_the_run_identity() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let register = register_of(workspace.root());
    let features = &register["1"]["parameters"];
    assert!(
        features.get("patient_column").is_some(),
        "the register does not record how the cohort was grouped: {features}"
    );
    assert!(
        features.get("network_directory").is_some(),
        "the register does not record which network was aggregated: {features}"
    );
}

// ---------------------------------------------------------------------------
// Helpers for the cohort-identity tests
// ---------------------------------------------------------------------------

/// One niche column, read back from every sample and concatenated.
fn labels_of(workspace: &Workspace, column: &str) -> Vec<f64> {
    let mut all = Vec::new();
    for sample in 1..=2 {
        let path = workspace
            .net_dir()
            .join(format!("nodes_patient-{sample}_sample-1.parquet"));
        let table = read_table(&path, Extension::Parquet).unwrap();
        all.extend(table.f64_column(column).unwrap());
    }
    all
}

/// The same configuration with a different edge rule in step 1.
fn with_edges_method(config: RawConfig, method: &str) -> RawConfig {
    edit(config, |yaml| {
        yaml.replace(
            "  Edges method: delaunay",
            &format!("  Edges method: {method}"),
        )
    })
}

/// The same configuration with a different number of clusters.
fn with_n_clusters(config: RawConfig, n: usize) -> RawConfig {
    edit(config, |yaml| {
        yaml.replace("    n_clusters: 3", &format!("    n_clusters: {n}"))
    })
}

/// The same configuration pointing step 3 at another network directory.
fn with_network_directory(config: RawConfig, directory: &str, extension: &str) -> RawConfig {
    edit(config, |yaml| {
        // Only the niche section's two keys; the tysserand section has neither.
        let niche = yaml.find("Niche Analysis:").expect("a niche section");
        let (head, tail) = yaml.split_at(niche);
        format!(
            "{head}{}",
            tail.replace(
                "  Network directory: Default",
                &format!("  Network directory: {directory}")
            )
            .replace("  Extension: parquet", &format!("  Extension: {extension}"))
        )
    })
}

/// Round-trip a configuration through YAML so a test can rewrite one key.
fn edit(config: RawConfig, change: impl FnOnce(String) -> String) -> RawConfig {
    RawConfig::from_yaml_str(&change(config.to_yaml_string().unwrap())).unwrap()
}

/// A two-sample cohort of CSV nodes files, returned as `(path, bytes)` so a
/// test can prove they were left alone.
fn write_csv_cohort(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut written = Vec::new();
    for sample in 1..=2 {
        let path = directory.join(format!("nodes_patient-{sample}_sample-1.csv"));
        let text = "X_position,Y_position,Cluster\n0.0,0.0,A\n1.0,0.0,B\n0.0,1.0,A\n";
        std::fs::write(&path, text).unwrap();
        written.push((path, text.as_bytes().to_vec()));
    }
    written
}

// ---------------------------------------------------------------------------
// Two runs at once
// ---------------------------------------------------------------------------

/// The blocking defect for any batch of runs: two analyses started together in
/// one working directory used to read an empty register, both take the number
/// `1-1-1`, and both write to that directory, that cache file and that label
/// column. One of the two was simply lost, and the directory could be left
/// holding the figures of one run beside the labels of the other.
///
/// Each must now come away with a number of its own.
#[test]
fn two_runs_at_once_do_not_take_the_same_number() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let root = workspace.root().to_path_buf();
    let handles: Vec<_> = [3usize, 2, 4]
        .into_iter()
        .map(|n| {
            let root = root.clone();
            std::thread::spawn(move || {
                let configuration = with_n_clusters(config(), n);
                niche_analysis(&configuration, &root, &SilentProgress, &NoFigures)
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap().expect("a concurrent run failed");
    }

    // Three distinct partitions of one projection of one aggregation.
    let register = register_of(workspace.root());
    let clusterings = &register["1"]["reduction"]["1"]["clustering"];
    let recorded = clusterings.as_object().expect("a clustering map");
    assert_eq!(
        recorded.len(),
        3,
        "three runs, {} recorded: {register}",
        recorded.len()
    );

    // Every one of them kept its own directory and its own label column.
    let mut seen: Vec<usize> = recorded
        .values()
        .map(|c| c["parameters"]["n_clusters"].as_u64().unwrap() as usize)
        .collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![2, 3, 4], "a run's settings were overwritten");

    for id in recorded.keys() {
        let column = format!("niches_1-1-{id}");
        assert_eq!(
            labels_of(&workspace, &column).len(),
            72,
            "{column} is missing or short"
        );
        assert!(
            workspace
                .root()
                .join(format!("Niche_Analysis/1-1-{id}/parameters.json"))
                .is_file(),
            "1-1-{id} has no parameters"
        );
    }
}

/// And the register itself survives: a run writing it while another reads it
/// must never leave behind something that cannot be parsed.
#[test]
fn the_register_stays_readable_under_concurrent_runs() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let root = workspace.root().to_path_buf();
    let handles: Vec<_> = (2..=5)
        .map(|n| {
            let root = root.clone();
            std::thread::spawn(move || {
                niche_analysis(
                    &with_n_clusters(config(), n),
                    &root,
                    &SilentProgress,
                    &NoFigures,
                )
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap().unwrap();
    }

    // Parses, and holds every run.
    let register = register_of(workspace.root());
    assert_eq!(
        register["1"]["reduction"]["1"]["clustering"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
}

// ---------------------------------------------------------------------------
// A number is an alias; the parameters are the identity
// ---------------------------------------------------------------------------

/// Losing the register used to recycle the numbers, so the next run took
/// `1-1-1` and overwrote the results of a run it had nothing to do with. The
/// directory now says what produced it, and a number that would land on
/// somebody else's results is passed over.
#[test]
fn a_recycled_number_does_not_overwrite_another_runs_results() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let first = with_order(config(), 2);
    niche_analysis(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    let kept = std::fs::read_to_string(
        workspace
            .root()
            .join("Niche_Analysis/1-1-1/parameters.json"),
    )
    .unwrap();

    // The register is gone; the directories beside it are not.
    std::fs::remove_file(workspace.root().join("Niche_Analysis/runs.json")).unwrap();

    let spoken = Spoken::default();
    niche_analysis(&config(), workspace.root(), &spoken, &NoFigures).unwrap();

    assert_eq!(
        std::fs::read_to_string(
            workspace
                .root()
                .join("Niche_Analysis/1-1-1/parameters.json")
        )
        .unwrap(),
        kept,
        "the earlier run's results were overwritten by an unrelated run"
    );
}

/// Running the very same settings twice is not that case: it is the same run,
/// it lands on the same directory, and it says so plainly.
#[test]
fn the_same_settings_twice_reuse_their_directory_and_say_so() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis(&config(), workspace.root(), &spoken, &NoFigures).unwrap();

    assert!(
        spoken.said("1-1-1") && spoken.said("same settings"),
        "a re-run of identical settings was not announced as one: {:?}",
        spoken.lines()
    );
    assert_eq!(
        run_directories(&workspace.root().join("Niche_Analysis")).len(),
        1
    );
}

// ---------------------------------------------------------------------------
// A run that fails leaves nothing to trip over
// ---------------------------------------------------------------------------

/// A run that dies part-way used to leave an empty numbered directory that the
/// register knew nothing about — and the next run to be handed that number was
/// told its settings "had already been run".
#[test]
fn a_failed_run_leaves_no_directory_behind() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // A clusterer with no implementation: refused before anything is written.
    let doomed = with_clusterer(config(), "ecg");
    assert!(niche_analysis(&doomed, workspace.root(), &SilentProgress, &NoFigures).is_err());

    assert_eq!(
        run_directories(&workspace.root().join("Niche_Analysis")),
        vec!["1-1-1".to_string()],
        "a failed run left a directory behind"
    );
}

/// And the next legitimate run is not warned about results that do not exist.
#[test]
fn a_run_after_a_failure_is_not_told_it_has_run_before() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    let _ = niche_analysis(
        &with_clusterer(config(), "ecg"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    );

    let spoken = Spoken::default();
    niche_analysis(
        &with_n_clusters(config(), 2),
        workspace.root(),
        &spoken,
        &NoFigures,
    )
    .unwrap();
    assert!(
        !spoken.said("same settings"),
        "a fresh run was announced as a repeat: {:?}",
        spoken.lines()
    );
}

/// Each sample of a per-sample run is recorded as it finishes, so a failure
/// half-way does not lose the ones that were completed.
#[test]
fn a_per_sample_run_records_each_sample_as_it_goes() {
    let workspace = Workspace::new(3, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let per_sample = with_processing_method(config(), "Per sample");
    niche_analysis(&per_sample, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // Each sample is still aggregated on its own, so each has its own caches.
    let register = register_of(workspace.root());
    let aggregations: Vec<&serde_json::Value> = register
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| key.as_str() != "per_sample_runs")
        .map(|(_, value)| value)
        .collect();
    assert_eq!(aggregations.len(), 3, "one aggregation per sample");
    for entry in aggregations {
        assert_eq!(entry["mode"], "per_sample");
        assert!(
            entry["sample"].as_str().is_some_and(|s| !s.is_empty()),
            "a per-sample aggregation does not say which sample: {entry}"
        );
        assert_eq!(entry["status"], "done");
    }

    // And all three belong to one run, which knows where their caches are.
    let run = &register["per_sample_runs"]["1"];
    assert_eq!(run["status"], "done");
    assert_eq!(run["path"], "ps-1");
    let samples = run["samples"].as_object().expect("a sample map");
    assert_eq!(samples.len(), 3, "{run}");
    for numbers in samples.values() {
        assert!(
            numbers
                .as_str()
                .is_some_and(|n| n.matches('-').count() == 2),
            "a sample does not name its cache numbers: {run}"
        );
    }
}

// ---------------------------------------------------------------------------
// What a run directory says about itself
// ---------------------------------------------------------------------------

/// `parameters.json` was the whole niche section, written identically into
/// every directory: an aggregated run and a per-sample one were byte for byte
/// the same file, and neither said which sample, which numbers or which
/// cohort it belonged to.
#[test]
fn a_run_directory_says_what_produced_it() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.root().join("Niche_Analysis/1-1-1/run.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(run["run"], "1-1-1");
    assert_eq!(run["mode"], "aggregated");
    assert_eq!(run["sample"], serde_json::Value::Null);
    assert!(run["cohort"].as_str().is_some(), "{run}");
    assert!(run["updated"].as_str().is_some(), "{run}");

    // The settings that were actually used, one sub-section rather than both.
    assert_eq!(run["parameters"]["clusterer_type"], "gmm");
    assert_eq!(run["parameters"]["n_clusters"], 3);
    assert!(
        run["parameters"].get("Per sample").is_none(),
        "the unused sub-section is still there: {run}"
    );
    // And the rendering settings, which do not change the partition.
    assert_eq!(run["render"]["normalize"], "total");
    assert_eq!(run["render"]["phenotype_column"], "Cluster");
}

/// A per-sample directory names its sample, which is what the report and the
/// interface need in order to file its figures under the right patient — and
/// names the run it belongs to rather than the numbers of its own caches,
/// which differ from one sample to the next.
#[test]
fn a_per_sample_run_directory_names_its_sample() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(
        &with_processing_method(config(), "Per sample"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            workspace
                .root()
                .join("Niche_Analysis/ps-1/patient-1_sample-1/run.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(run["mode"], "per_sample");
    assert_eq!(run["sample"], "patient-1_sample-1");
    assert_eq!(run["run"], "ps-1");
    // The cache numbers are still recorded, just not as the run's name.
    assert_eq!(run["numbers"], "1-1-1");
}

// ---------------------------------------------------------------------------
// Rendering settings are recorded, not renumbered
// ---------------------------------------------------------------------------

/// `normalize` picks which composition figures are drawn; it changes nothing
/// about the partition, so it must not make a new run. It must still be
/// recorded — the directory used to accumulate figures from several
/// normalisations while `parameters.json` named only the last.
#[test]
fn changing_the_normalisation_records_a_rendering_rather_than_a_run() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    let clr = with_normalize(config(), "clr");
    niche_analysis(&clr, workspace.root(), &spoken, &NoFigures).unwrap();

    // One run, not two.
    assert_eq!(
        run_directories(&workspace.root().join("Niche_Analysis")).len(),
        1
    );
    assert!(
        !spoken.said("overwritten"),
        "adding a rendering was announced as an overwrite: {:?}",
        spoken.lines()
    );

    // And both renderings are on the record.
    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.root().join("Niche_Analysis/1-1-1/run.json")).unwrap(),
    )
    .unwrap();
    let renderings: Vec<&str> = run["renderings"]
        .as_array()
        .expect("a list of renderings")
        .iter()
        .map(|r| r["normalize"].as_str().unwrap())
        .collect();
    assert!(renderings.contains(&"total"), "{run}");
    assert!(renderings.contains(&"clr"), "{run}");
}

/// The same configuration with a different neighbourhood order.
fn with_order(config: RawConfig, order: usize) -> RawConfig {
    edit(config, |yaml| {
        yaml.replace("    order: '1'", &format!("    order: '{order}'"))
    })
}

/// The same configuration with a different clusterer.
fn with_clusterer(config: RawConfig, clusterer: &str) -> RawConfig {
    edit(config, |yaml| {
        yaml.replace(
            "    clusterer_type: gmm",
            &format!("    clusterer_type: {clusterer}"),
        )
    })
}

/// The same configuration with a different normalisation.
fn with_normalize(config: RawConfig, normalize: &str) -> RawConfig {
    edit(config, |yaml| {
        yaml.replace(
            "    normalize: total",
            &format!("    normalize: {normalize}"),
        )
    })
}

/// The same configuration run over the pooled cohort, per sample, or both.
fn with_processing_method(config: RawConfig, method: &str) -> RawConfig {
    edit(config, |yaml| {
        yaml.replace(
            "  Processing method: Aggregated nodes",
            &format!("  Processing method: {method}"),
        )
    })
}

/// Clearing the temporary directory takes the networks with it, so a niche
/// analysis run straight afterwards has nothing to read. It used to fail with
/// `failed to read .../temp/net_dir_mosna: No such file or directory (os error
/// 2)` — a message that names a path the user never chose and does not say what
/// to do about it.
#[test]
fn a_niche_analysis_without_a_network_says_to_run_step_one() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    clear_temporary(workspace.root(), &SilentProgress).unwrap();

    let error = niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures)
        .expect_err("a missing network directory was not reported");

    let message = error.to_string();
    assert!(message.contains("Tysserand"), "{message}");
    assert!(message.contains("net_dir_mosna"), "{message}");
}

/// A custom network directory that does not exist is a different mistake —
/// the user typed a path — so it is not answered with "run step 1".
#[test]
fn a_missing_custom_network_directory_is_reported_as_a_path() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    let configuration = with_network_directory(config(), "not_here", "parquet");

    let error = niche_analysis(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .expect_err("a missing custom directory was not reported");

    let message = error.to_string();
    assert!(message.contains("not_here"), "{message}");
    assert!(
        !message.contains("Tysserand"),
        "a path the user typed was blamed on step 1: {message}"
    );
}

// ---------------------------------------------------------------------------
// A per-sample run is one run
// ---------------------------------------------------------------------------

/// Every sample of a per-sample run used to be numbered as its own
/// aggregation, so one run wrote `niches_1-1-1` into the first sample,
/// `niches_2-1-1` into the second and so on. There was no column the samples
/// shared, so the network view could not be told "colour by this run"; there
/// was no directory the run owned, so its four sets of figures sat beside the
/// aggregated runs indistinguishable from them; and there was no name a sweep
/// could use to refer to the run at all.
#[test]
fn a_per_sample_run_writes_one_column_across_every_sample() {
    let workspace = Workspace::new(3, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(
        &with_processing_method(config(), "Per sample"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    for sample in 1..=3 {
        let path = workspace
            .net_dir()
            .join(format!("nodes_patient-{sample}_sample-1.parquet"));
        let table = read_table(&path, Extension::Parquet).unwrap();
        assert!(
            table.has_column("niches_ps-1"),
            "{path:?} has no niches_ps-1 column, only {:?}",
            table.column_names()
        );
        assert_eq!(table.f64_column("niches_ps-1").unwrap().len(), 36);
    }
}

/// And it owns one directory, with a sub-directory per sample — which is the
/// shape the report already expects when it files a figure under a patient.
#[test]
fn a_per_sample_run_owns_one_directory_holding_its_samples() {
    let workspace = Workspace::new(3, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(
        &with_processing_method(config(), "Per sample"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let niche_dir = workspace.root().join("Niche_Analysis");
    assert_eq!(run_directories(&niche_dir), vec!["ps-1".to_string()]);

    let mut samples = run_directories(&niche_dir.join("ps-1"));
    samples.sort();
    assert_eq!(
        samples,
        vec![
            "patient-1_sample-1".to_string(),
            "patient-2_sample-1".to_string(),
            "patient-3_sample-1".to_string(),
        ]
    );

    // Each sample directory says which sample it is.
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(niche_dir.join("ps-1/patient-2_sample-1/run.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["mode"], "per_sample");
    assert_eq!(record["sample"], "patient-2_sample-1");
    assert_eq!(record["run"], "ps-1");
}

/// A second per-sample run with different settings is a second run, not four
/// more aggregations tacked onto the first.
#[test]
fn a_second_per_sample_run_gets_its_own_identity() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let first = with_processing_method(config(), "Per sample");
    niche_analysis(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    let second = with_n_clusters(first.clone(), 2);
    niche_analysis(&second, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let mut runs = run_directories(&workspace.root().join("Niche_Analysis"));
    runs.sort();
    assert_eq!(runs, vec!["ps-1".to_string(), "ps-2".to_string()]);

    // Both columns are in every sample, so the two runs can be compared.
    for sample in 1..=2 {
        let table = read_table(
            workspace
                .net_dir()
                .join(format!("nodes_patient-{sample}_sample-1.parquet")),
            Extension::Parquet,
        )
        .unwrap();
        assert!(table.has_column("niches_ps-1"));
        assert!(table.has_column("niches_ps-2"));
    }
}

/// Re-running identical per-sample settings is the same run: it keeps its
/// directory and its column rather than opening a second one.
#[test]
fn identical_per_sample_settings_are_the_same_run() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let per_sample = with_processing_method(config(), "Per sample");
    niche_analysis(&per_sample, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&per_sample, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    assert_eq!(
        run_directories(&workspace.root().join("Niche_Analysis")),
        vec!["ps-1".to_string()]
    );
}

/// The caches stay per sample — each sample has its own feature table, its own
/// projection and its own partition — so a second run that only re-clusters
/// still reuses the projections of all of them.
#[test]
fn a_per_sample_run_still_caches_each_sample_separately() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let per_sample = with_processing_method(config(), "Per sample");
    niche_analysis(&per_sample, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis(
        &with_n_clusters(per_sample, 2),
        workspace.root(),
        &spoken,
        &NoFigures,
    )
    .unwrap();

    let reused = spoken
        .lines()
        .iter()
        .filter(|line| line.contains("Reusing the cached projection"))
        .count();
    assert_eq!(
        reused, 2,
        "one projection per sample should have been reused"
    );
}

/// The aggregated mode is untouched: it still numbers its runs `a-r-c`.
#[test]
fn the_aggregated_mode_keeps_its_numbering() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(
        &with_processing_method(config(), "Both"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let mut runs = run_directories(&workspace.root().join("Niche_Analysis"));
    runs.sort();
    assert_eq!(runs, vec!["1-1-1".to_string(), "ps-1".to_string()]);
}

// ---------------------------------------------------------------------------
// Plot Network
// ---------------------------------------------------------------------------

/// `Plot Network`, `X coordinates column for niches` and `Y coordinates column
/// for niches` were three settings the pipeline never read:
/// `should_plot_network()` was called from nowhere in the whole repository. The
/// interface offered them, the configuration carried them, and turning them on
/// produced nothing.
#[test]
fn plot_network_draws_one_network_per_sample_coloured_by_niche() {
    let workspace = Workspace::new(3, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let drawn = CountingFigures::default();
    niche_analysis(
        &with_plot_network(config(), true),
        workspace.root(),
        &SilentProgress,
        &drawn,
    )
    .unwrap();

    assert_eq!(
        drawn.networks(),
        3,
        "one network per sample, coloured by its niche"
    );
}

/// And it stays off when it is off.
#[test]
fn plot_network_draws_nothing_when_it_is_not_asked_for() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let drawn = CountingFigures::default();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &drawn).unwrap();
    assert_eq!(drawn.networks(), 0);
}

/// Asking for it without saying where the cells are cannot draw anything, and
/// must not stop the analysis either — the niches are still computed.
#[test]
fn plot_network_without_coordinates_is_skipped_and_said_so() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let configuration = edit(with_plot_network(config(), true), |yaml| {
        yaml.replace(
            "  X coordinates column for niches: X_position",
            "  X coordinates column for niches: null",
        )
    });
    let drawn = CountingFigures::default();
    let spoken = Spoken::default();
    niche_analysis(&configuration, workspace.root(), &spoken, &drawn).unwrap();

    assert_eq!(drawn.networks(), 0);
    assert!(
        spoken.said("coordinates"),
        "nothing explained why no network was drawn: {:?}",
        spoken.lines()
    );
    // The analysis itself still ran.
    assert_eq!(labels_of(&workspace, "niches_1-1-1").len(), 72);
}

/// A per-sample run draws its networks too, one per sample, into the sample's
/// own directory.
#[test]
fn plot_network_works_for_a_per_sample_run() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let drawn = CountingFigures::default();
    niche_analysis(
        &with_processing_method(with_plot_network(config(), true), "Per sample"),
        workspace.root(),
        &SilentProgress,
        &drawn,
    )
    .unwrap();

    assert_eq!(drawn.networks(), 2);
}

/// The same configuration with `Plot Network` on or off.
fn with_plot_network(config: RawConfig, on: bool) -> RawConfig {
    edit(config, |yaml| {
        yaml.replace("  Plot Network: false", &format!("  Plot Network: {on}"))
    })
}

// ---------------------------------------------------------------------------
// Defects found while reviewing the fixes themselves
// ---------------------------------------------------------------------------

/// A per-sample run that fails must say so. The outcome of each sample is
/// propagated with `?`, which returns before the run's own status can be set —
/// so a failed run was left saying `running` for ever, and a batch resuming
/// from the register would wait on it or skip it wrongly.
#[test]
fn a_per_sample_run_that_fails_is_not_left_running() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // A phenotype column nothing has: the run gets as far as the composition
    // of its first sample and fails there.
    let doomed = edit(with_processing_method(config(), "Per sample"), |yaml| {
        let at = yaml.find("Niche Analysis:").unwrap();
        let (head, tail) = yaml.split_at(at);
        format!(
            "{head}{}",
            tail.replace("  Phenotype column: Cluster", "  Phenotype column: Absent")
        )
    });
    assert!(niche_analysis(&doomed, workspace.root(), &SilentProgress, &NoFigures).is_err());

    let register = register_of(workspace.root());
    assert_eq!(
        register["per_sample_runs"]["1"]["status"], "failed",
        "a run that failed still claims to be running: {register}"
    );
}

/// The per-sample equivalent of a recycled number. Losing the register starts
/// the `ps-` numbering again, and `ps-1` already holds another run's results:
/// they were overwritten, exactly as the numbered runs used to be.
#[test]
fn a_recycled_per_sample_number_does_not_overwrite_another_run() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let first = with_order(with_processing_method(config(), "Per sample"), 2);
    niche_analysis(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    let kept = std::fs::read_to_string(
        workspace
            .root()
            .join("Niche_Analysis/ps-1/patient-1_sample-1/run.json"),
    )
    .unwrap();

    std::fs::remove_file(workspace.root().join("Niche_Analysis/runs.json")).unwrap();

    let second = with_processing_method(config(), "Per sample");
    niche_analysis(&second, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    assert_eq!(
        std::fs::read_to_string(
            workspace
                .root()
                .join("Niche_Analysis/ps-1/patient-1_sample-1/run.json")
        )
        .unwrap(),
        kept,
        "the earlier per-sample run's results were overwritten"
    );
}

/// A per-sample run owns its directory, so that directory has to say what it
/// is — which is also what lets a claim tell it apart from another run's.
#[test]
fn a_per_sample_run_directory_describes_the_whole_run() {
    let workspace = Workspace::new(3, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(
        &with_processing_method(config(), "Per sample"),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.root().join("Niche_Analysis/ps-1/run.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(run["run"], "ps-1");
    assert_eq!(run["mode"], "per_sample");
    assert_eq!(run["status"], "done");
    assert!(run["fingerprint"].as_str().is_some(), "{run}");
    assert_eq!(run["parameters"]["clusterer_type"], "gmm");
    let samples = run["samples"].as_array().expect("the samples it covers");
    assert_eq!(samples.len(), 3, "{run}");
}

/// The scratch file an atomic write leaves beside its target must not be
/// mistaken for a sample. It is named `nodes_….parquet.<pid>.tmp`, which starts
/// with `nodes_` — and a discovery that matched on the prefix alone would try
/// to read a half-written parquet as a cohort member.
#[test]
fn an_atomic_write_scratch_file_is_not_taken_for_a_sample() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // What `write_parquet_atomic` would leave if it were interrupted.
    std::fs::write(
        workspace
            .net_dir()
            .join("nodes_patient-1_sample-1.parquet.99999.tmp"),
        b"half a parquet",
    )
    .unwrap();

    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures)
        .expect("a leftover scratch file broke the analysis");
    assert_eq!(labels_of(&workspace, "niches_1-1-1").len(), 72);
}

/// A queue of figure specifications is discarded once they are drawn, but a
/// run killed before that leaves one behind. `clear-temporary` is what removes
/// what a run leaves behind, so it has to take those too — a sweep that
/// crashes repeatedly would otherwise fill the working directory with queues
/// nothing will ever draw.
#[test]
fn clearing_removes_an_abandoned_figure_queue() {
    let dir = tempfile::tempdir().unwrap();
    let orphan = dir.path().join(".mosna-figures/12345-0/00000-histogram");
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(orphan.join("figure.json"), b"{}").unwrap();

    clear_temporary(dir.path(), &SilentProgress).unwrap();

    assert!(
        !dir.path().join(".mosna-figures").exists(),
        "an abandoned figure queue survived"
    );
}

/// In `Both` mode the aggregated half and the per-sample half are two runs in
/// one command. A failure in the second must not leave the first unrecorded,
/// nor the second claiming to be running.
#[test]
fn a_failure_in_the_per_sample_half_leaves_the_aggregated_half_recorded() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let doomed = edit(with_processing_method(config(), "Both"), |yaml| {
        let at = yaml.find("Niche Analysis:").unwrap();
        let (head, tail) = yaml.split_at(at);
        format!(
            "{head}{}",
            tail.replace("  Phenotype column: Cluster", "  Phenotype column: Absent")
        )
    });
    assert!(niche_analysis(&doomed, workspace.root(), &SilentProgress, &NoFigures).is_err());

    let register = register_of(workspace.root());
    // The aggregated half failed first, on its own composition, and says so.
    assert_eq!(register["1"]["status"], "failed", "{register}");
    assert!(
        register
            .as_object()
            .unwrap()
            .values()
            .all(|entry| entry["status"] != "running"),
        "something is still claiming to run: {register}"
    );
}

/// A number passed over because its directory belongs to someone else must not
/// stay in the register. It was left there marked `running`, so a register read
/// back after a recycled number showed a run that no process was working on and
/// no directory belonged to — and a batch resuming from the register would wait
/// on it for ever.
#[test]
fn a_number_that_was_passed_over_leaves_no_entry_behind() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let first = with_order(with_processing_method(config(), "Per sample"), 2);
    niche_analysis(&first, workspace.root(), &SilentProgress, &NoFigures).unwrap();
    std::fs::remove_file(workspace.root().join("Niche_Analysis/runs.json")).unwrap();

    let second = with_processing_method(config(), "Per sample");
    niche_analysis(&second, workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let register = register_of(workspace.root());
    let runs = register["per_sample_runs"].as_object().unwrap();
    assert_eq!(
        runs.len(),
        1,
        "a passed-over number was left in the register: {register}"
    );
    for entry in runs.values() {
        assert_ne!(entry["status"], "running", "{register}");
        assert_eq!(entry["path"], "ps-2", "the surviving entry is the one used");
    }
}

/// The same for a numbered run: the clustering ids it stepped over are not
/// recorded as attempts that were never made.
#[test]
fn a_numbered_run_that_was_passed_over_leaves_no_entry_behind() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    niche_analysis(
        &with_order(config(), 2),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();
    std::fs::remove_file(workspace.root().join("Niche_Analysis/runs.json")).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let register = register_of(workspace.root());
    for (_, aggregation) in register.as_object().unwrap() {
        let Some(reductions) = aggregation["reduction"].as_object() else {
            continue;
        };
        for reduction in reductions.values() {
            for clustering in reduction["clustering"].as_object().unwrap().values() {
                assert_ne!(
                    clustering["status"], "running",
                    "a passed-over partition was left in the register: {register}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// What a parameter sweep does to the edges of an interval
// ---------------------------------------------------------------------------

/// `n_clusters` is clamped to the number of cells inside the clusterer, so
/// every value at or above the cohort size computes the same partition — but
/// each is recorded as a run of its own, and each pays the full cost of
/// computing it. A sweep whose upper bound overshoots therefore fills the
/// register with duplicates it cannot tell apart.
///
/// The effective value is what identifies the run, the way `k_cluster` already
/// records the value `n_neighbors` caps it to.
#[test]
fn a_cluster_count_beyond_the_cohort_is_the_run_it_is_clamped_to() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // 72 cells in the cohort; both of these clamp to 72.
    niche_analysis(
        &with_n_clusters(config(), 500),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let spoken = Spoken::default();
    niche_analysis(
        &with_n_clusters(config(), 900),
        workspace.root(),
        &spoken,
        &NoFigures,
    )
    .unwrap();

    assert_eq!(
        run_directories(&workspace.root().join("Niche_Analysis")).len(),
        1,
        "two settings that compute the same partition opened two runs"
    );
    assert!(
        spoken.said("Reusing the cached partition"),
        "the identical partition was computed a second time: {:?}",
        spoken.lines()
    );
}

/// And the register records what was actually asked of the clusterer, so a
/// reader comparing two runs is not told they differ on a number neither of
/// them used.
#[test]
fn the_register_records_the_cluster_count_that_was_used() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(
        &with_n_clusters(config(), 500),
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let register = register_of(workspace.root());
    let clustering = &register["1"]["reduction"]["1"]["clustering"]["1"]["parameters"];
    assert_eq!(
        clustering["n_clusters"], 72,
        "the register claims a cluster count the clusterer never used: {clustering}"
    );
}

// ---------------------------------------------------------------------------
// What a sweep needs to read back about a run
// ---------------------------------------------------------------------------

/// The one number every run of a sweep is compared on, and it was nowhere on
/// disk: to learn how many niches a run found you had to open its label column
/// and count the distinct values. A comparison table over two hundred runs
/// cannot be built that way.
#[test]
fn a_run_records_how_many_niches_it_found() {
    let workspace = Workspace::new(2, 36, &["A", "B", "C"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.root().join("Niche_Analysis/1-1-1/run.json")).unwrap(),
    )
    .unwrap();

    let found = run["result"]["niches"]
        .as_u64()
        .unwrap_or_else(|| panic!("a run does not say how many niches it found: {run}"));
    let distinct = labels_of(&workspace, "niches_1-1-1")
        .into_iter()
        .map(|v| v as u64)
        .collect::<std::collections::BTreeSet<_>>()
        .len() as u64;
    assert_eq!(found, distinct);

    // And how big they are, which is what says whether a partition is one
    // niche and a scattering of singletons.
    let sizes = run["result"]["sizes"].as_array().expect("niche sizes");
    assert_eq!(sizes.len() as u64, found);
    assert_eq!(
        sizes.iter().map(|s| s.as_u64().unwrap()).sum::<u64>(),
        72,
        "the niche sizes do not account for every cell: {run}"
    );
}

/// Leiden cannot merge across a connected component, so the number of
/// components is a floor on the niche count that no `resolution` can go below.
/// On a real cohort that floor was 418 — a sweep of `resolution` over five
/// orders of magnitude moved the answer from 418 to 609 and never explained
/// why it would not go lower.
#[test]
fn a_leiden_run_reports_the_floor_its_graph_imposes() {
    let workspace = Workspace::new(2, 36, &["A", "B", "C"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis(
        &with_clusterer(config(), "leiden"),
        workspace.root(),
        &spoken,
        &NoFigures,
    )
    .unwrap();

    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.root().join("Niche_Analysis/1-1-1/run.json")).unwrap(),
    )
    .unwrap();
    let components = run["result"]["graph_components"]
        .as_u64()
        .unwrap_or_else(|| panic!("a leiden run does not report its graph: {run}"));
    let niches = run["result"]["niches"].as_u64().unwrap();

    assert!(components >= 1);
    assert!(
        niches >= components,
        "fewer niches ({niches}) than components ({components}) is impossible"
    );
    assert!(
        spoken.said("component"),
        "nothing said what the graph looks like: {:?}",
        spoken.lines()
    );
}

/// A clusterer that partitions a matrix rather than a graph has no such floor,
/// and must not claim one.
#[test]
fn a_clusterer_without_a_graph_reports_no_components() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let run: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace.root().join("Niche_Analysis/1-1-1/run.json")).unwrap(),
    )
    .unwrap();
    assert!(
        run["result"]
            .get("graph_components")
            .is_none_or(|v| v.is_null()),
        "gmm claims a graph it never built: {run}"
    );
}

// ---------------------------------------------------------------------------
// What the progress bar is counting
// ---------------------------------------------------------------------------

/// Step 3 used to report three steps, with the reduction and the clustering
/// merged into the second. They are the two stages whose cost differs most —
/// a projection is minutes, a partition is seconds — so a bar that lumps them
/// together sits still through the expensive one and cannot say which is
/// running.
#[test]
fn the_reduction_and_the_clustering_are_counted_apart() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let steps = RecordingSteps::default();
    niche_analysis(&config(), workspace.root(), &steps, &NoFigures).unwrap();

    let seen = steps.descriptions();
    for stage in ["aggregation", "reduction", "clustering"] {
        assert!(
            seen.iter().any(|d| d.to_lowercase().contains(stage)),
            "no step announced the {stage}: {seen:?}"
        );
    }

    // And every step of one run counts against the same total.
    let totals: std::collections::BTreeSet<usize> =
        steps.steps().iter().map(|(_, total, _)| *total).collect();
    assert_eq!(totals.len(), 1, "the total moved mid-run: {totals:?}");
    assert!(
        *totals.iter().next().unwrap() >= 4,
        "the stages were not counted apart"
    );
}

/// A run without a reduction has no reduction stage to wait for, and must not
/// leave a quarter of the bar unexplained.
#[test]
fn a_run_without_a_reduction_counts_one_stage_fewer() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    let configuration = config_with_reducer("none");
    tysserand_network(
        &configuration,
        workspace.root(),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let steps = RecordingSteps::default();
    niche_analysis(&configuration, workspace.root(), &steps, &NoFigures).unwrap();

    let last = steps.steps().last().cloned().expect("some progress");
    assert_eq!(last.0, last.1, "the bar did not reach its total");
    assert!(
        !steps
            .descriptions()
            .iter()
            .any(|d| d.to_lowercase().contains("reduction")),
        "a run with no reduction announced one"
    );
}

/// Records the progress steps a run reports, so a test can assert on the bar
/// rather than on the log.
#[derive(Default)]
struct RecordingSteps(std::sync::Mutex<Vec<(usize, usize, String)>>);

impl mosna_pipeline::progress::Progress for RecordingSteps {
    fn info(&self, _message: &str) {}
    fn step(&self, current: usize, total: usize, description: &str) {
        self.0
            .lock()
            .unwrap()
            .push((current, total, description.to_string()));
    }
}

impl RecordingSteps {
    fn steps(&self) -> Vec<(usize, usize, String)> {
        self.0.lock().unwrap().clone()
    }
    fn descriptions(&self) -> Vec<String> {
        self.steps().into_iter().map(|(_, _, d)| d).collect()
    }
}

// ---------------------------------------------------------------------------
// Where a run's results are written
// ---------------------------------------------------------------------------

/// A sweep of two hundred runs would bury the handful a user started
/// deliberately, so its results go in a sub-directory of their own. The
/// register does not move with them — see the test below for why.
#[test]
fn a_run_can_write_its_results_into_a_sub_directory() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    niche_analysis_in(
        &config(),
        workspace.root(),
        Some("sensitivity_analysis"),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let niche_dir = workspace.root().join("Niche_Analysis");
    assert!(
        niche_dir
            .join("sensitivity_analysis/1-1-1/run.json")
            .is_file(),
        "the run did not land in its sub-directory: {:?}",
        run_directories(&niche_dir)
    );
    assert!(
        !niche_dir.join("1-1-1").exists(),
        "the run also landed at the top level"
    );

    // The register stays where every run's register is.
    assert!(niche_dir.join("runs.json").is_file());
    assert!(!niche_dir.join("sensitivity_analysis/runs.json").exists());
}

/// The intermediate files do not move: they are named by the run's numbers and
/// shared with every other run that computes the same stage, which is what lets
/// a sweep read back what a hand-started run already produced.
#[test]
fn a_run_in_a_sub_directory_shares_the_intermediate_files() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    // By hand first, then the same settings as part of a sweep.
    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    let spoken = Spoken::default();
    niche_analysis_in(
        &config(),
        workspace.root(),
        Some("sensitivity_analysis"),
        &spoken,
        &NoFigures,
    )
    .unwrap();

    for stage in ["aggregated features", "projection", "partition"] {
        assert!(
            spoken.said(&format!("Reusing the cached {stage}")),
            "the sweep recomputed the {stage} the first run had already done"
        );
    }
    assert!(
        workspace
            .root()
            .join("temp/intermediate_files/var_aggreg-1/var_aggreg_1.parquet")
            .is_file(),
        "the intermediate files moved with the results"
    );
}

/// The reason the register stays put: run numbers name the label column written
/// into every nodes file. A sweep numbering from one alongside hand-started
/// runs would write a second, different `niches_1-1-1` over the first.
#[test]
fn a_sub_directory_does_not_restart_the_numbering() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    niche_analysis(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis_in(
        &with_n_clusters(config(), 2),
        workspace.root(),
        Some("sensitivity_analysis"),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let niche_dir = workspace.root().join("Niche_Analysis");
    assert!(niche_dir.join("1-1-1").is_dir(), "the hand-started run");
    assert!(
        niche_dir.join("sensitivity_analysis/1-1-2").is_dir(),
        "the swept run took the next number, not the first: {:?}",
        run_directories(&niche_dir.join("sensitivity_analysis"))
    );

    // Two distinct label columns, so the two partitions can be told apart.
    let table = read_table(
        workspace.net_dir().join("nodes_patient-1_sample-1.parquet"),
        Extension::Parquet,
    )
    .unwrap();
    assert!(table.has_column("niches_1-1-1"));
    assert!(table.has_column("niches_1-1-2"));
}

/// A per-sample sweep lands in the sub-directory too, keeping its own shape.
#[test]
fn a_per_sample_run_lands_in_the_sub_directory_as_well() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();

    niche_analysis_in(
        &with_processing_method(config(), "Per sample"),
        workspace.root(),
        Some("sensitivity_analysis"),
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    let run = workspace
        .root()
        .join("Niche_Analysis/sensitivity_analysis/ps-1");
    assert!(run.join("run.json").is_file(), "{run:?}");
    assert!(run.join("patient-1_sample-1/run.json").is_file());
}

/// Without a sub-directory nothing changes: a run started from the action bar
/// writes where it always has.
#[test]
fn a_run_without_a_sub_directory_writes_where_it_always_has() {
    let workspace = Workspace::new(2, 36, &["A", "B"]);
    tysserand_network(&config(), workspace.root(), &SilentProgress, &NoFigures).unwrap();
    niche_analysis_in(
        &config(),
        workspace.root(),
        None,
        &SilentProgress,
        &NoFigures,
    )
    .unwrap();

    assert!(workspace
        .root()
        .join("Niche_Analysis/1-1-1/run.json")
        .is_file());
}
