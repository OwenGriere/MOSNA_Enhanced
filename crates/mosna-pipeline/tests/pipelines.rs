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
        spoken.said("already run as Niche_Analysis/1-1-1"),
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

/// Re-running the same settings warns, names the directory it is about to
/// overwrite, and does not open a third one.
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
        spoken.said("already run as Niche_Analysis/1-1-1"),
        "no warning naming the directory"
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
