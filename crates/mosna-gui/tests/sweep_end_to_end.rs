//! A sweep, run for real, against the pipeline it drives.
//!
//! The unit tests say the plan is the product of its axes and that the order is
//! the one the cache rewards. This says the two actually meet: the
//! configurations a plan produces are run, in the order the plan gives them,
//! and the intermediate files are read back exactly where they should be.
//!
//! That is the requirement the sensitivity screen is built on. A sweep that
//! recomputed the aggregation for every point of its grid would be correct and
//! useless — 14 s a run instead of 0.4 on a real cohort.

use std::path::{Path, PathBuf};

use mosna_config::RawConfig;
use mosna_gui::model::sweep::{self, Axis, ClustererSweep, Scale, Values};
use mosna_pipeline::{niche_analysis, tysserand_network, NoFigures};

/// Records what the pipeline says, so a test can count cache hits.
#[derive(Default)]
struct Spoken(std::sync::Mutex<Vec<String>>);

impl mosna_pipeline::progress::Progress for Spoken {
    fn info(&self, message: &str) {
        self.0.lock().unwrap().push(message.to_string());
    }
    fn step(&self, _current: usize, _total: usize, _description: &str) {}
}

impl Spoken {
    fn count(&self, fragment: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|line| line.contains(fragment))
            .count()
    }
}

/// A cohort of three samples, and a configuration pointing at it.
fn workspace() -> (tempfile::TempDir, PathBuf, RawConfig) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let raw = root.join("raw");
    std::fs::create_dir_all(&raw).unwrap();

    for sample in 1..=3 {
        let n = 36usize;
        let side = 6usize;
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let mut labels = Vec::new();
        for i in 0..n {
            xs.push((i / side) as f64 + ((i * 7) % 5) as f64 * 0.03);
            ys.push((i % side) as f64 + ((i * 11) % 5) as f64 * 0.03);
            labels.push(["A", "B", "C"][i % 3]);
        }
        let table = mosna_io::Table::from_columns(vec![
            ("X_position".into(), mosna_io::Table::f64_array(xs)),
            ("Y_position".into(), mosna_io::Table::f64_array(ys)),
            ("Cluster".into(), mosna_io::Table::string_array(labels)),
        ])
        .unwrap();
        mosna_io::write::write_parquet::write_parquet(
            &table,
            raw.join(format!("nodes_patient-{sample}_sample-1.parquet")),
        )
        .unwrap();
    }

    let config = RawConfig::from_yaml_str(include_str!("sweep_e2e_config.yaml")).unwrap();
    (dir, root, config)
}

fn interval(key: &str, from: f64, to: f64, count: usize, integer: bool) -> Axis {
    Axis {
        key: key.to_string(),
        stage: sweep::stage_of(key),
        values: Values::Interval {
            from,
            to,
            count,
            scale: Scale::Linear,
            integer,
        },
    }
}

/// Run every configuration a plan produces, in the plan's order.
fn run_plan(root: &Path, base: &RawConfig, plan: &sweep::Plan) -> Spoken {
    let spoken = Spoken::default();
    for combination in &plan.combinations {
        let configuration = sweep::apply(base, combination, sweep::AGGREGATED);
        niche_analysis(&configuration, root, &spoken, &NoFigures)
            .unwrap_or_else(|e| panic!("{:?} failed: {e}", combination.settings));
    }
    spoken
}

/// The requirement the screen is built on: a grid that varies only the
/// clustering computes its aggregation and its projection once, and reads them
/// back for every run after the first.
#[test]
fn a_sweep_of_the_clustering_recomputes_nothing_above_it() {
    let (_dir, root, config) = workspace();
    tysserand_network(&config, &root, &mosna_pipeline::SilentProgress, &NoFigures).unwrap();

    let gmm = ClustererSweep {
        clusterer: "gmm".into(),
        axes: vec![interval("n_clusters", 2.0, 5.0, 4, true)],
    };
    let plan = sweep::plan(&[], &[gmm]);
    assert_eq!(plan.len(), 4);
    assert_eq!(plan.aggregations(), 1, "one aggregation for the whole grid");

    let spoken = run_plan(&root, &config, &plan);

    // The first run computes them; the three after it read them back.
    assert_eq!(
        spoken.count("Reusing the cached aggregated features"),
        3,
        "the aggregation was recomputed during the sweep"
    );
    assert_eq!(
        spoken.count("Reusing the cached projection"),
        3,
        "the projection was recomputed during the sweep"
    );
    assert_eq!(
        spoken.count("Reusing the cached partition"),
        0,
        "the partitions are what the sweep is varying"
    );

    // And four runs landed, each with its own labels.
    let niche_dir = root.join("Niche_Analysis");
    let mut runs: Vec<String> = std::fs::read_dir(&niche_dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    runs.sort();
    assert_eq!(runs, vec!["1-1-1", "1-1-2", "1-1-3", "1-1-4"]);

    // The labels are written into the nodes files under the ordinary name, so a
    // swept run is indistinguishable from one started by hand.
    let nodes = mosna_io::read::get_opener::read_table(
        root.join("temp/net_dir_mosna/nodes_patient-1_sample-1.parquet"),
        mosna_io::read::get_opener::Extension::Parquet,
    )
    .unwrap();
    for run in &runs {
        assert!(
            nodes.has_column(&format!("niches_{run}")),
            "niches_{run} is missing: {:?}",
            nodes.column_names()
        );
    }
}

/// A grid that varies the aggregation as well recomputes only what sits under
/// the change — which is what the order of the product is for.
#[test]
fn a_sweep_recomputes_a_stage_only_when_the_stage_above_it_moves() {
    let (_dir, root, config) = workspace();
    tysserand_network(&config, &root, &mosna_pipeline::SilentProgress, &NoFigures).unwrap();

    let shared = vec![interval("order", 1.0, 2.0, 2, true)];
    let gmm = ClustererSweep {
        clusterer: "gmm".into(),
        axes: vec![interval("n_clusters", 2.0, 4.0, 3, true)],
    };
    let plan = sweep::plan(&shared, &[gmm]);

    assert_eq!(plan.len(), 6);
    assert_eq!(plan.aggregations(), 2, "one per neighbourhood order");

    let spoken = run_plan(&root, &config, &plan);

    // Six runs, two aggregations: four of them read the features back.
    assert_eq!(spoken.count("Reusing the cached aggregated features"), 4);
    assert_eq!(spoken.count("Reusing the cached projection"), 4);

    // Two aggregations on disk, exactly as the plan said.
    let caches = root.join("temp/intermediate_files");
    let aggregations = std::fs::read_dir(&caches).unwrap().flatten().count();
    assert_eq!(aggregations, 2, "{:?}", caches);
}

/// Running the same grid twice costs nothing the second time: every stage is
/// already on disk, which is what lets a sweep be resumed after a stop.
#[test]
fn a_sweep_run_again_reads_everything_back() {
    let (_dir, root, config) = workspace();
    tysserand_network(&config, &root, &mosna_pipeline::SilentProgress, &NoFigures).unwrap();

    let gmm = ClustererSweep {
        clusterer: "gmm".into(),
        axes: vec![interval("n_clusters", 2.0, 4.0, 3, true)],
    };
    let plan = sweep::plan(&[], &[gmm]);

    run_plan(&root, &config, &plan);
    let again = run_plan(&root, &config, &plan);

    assert_eq!(
        again.count("Reusing the cached partition"),
        3,
        "every partition"
    );
    assert_eq!(again.count("Reusing the cached aggregated features"), 3);
    assert_eq!(
        std::fs::read_dir(root.join("Niche_Analysis"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_dir())
            .count(),
        3,
        "the second pass opened new runs instead of landing on the first's"
    );
}

/// Every run a sweep produces records what it found, which is what the
/// comparison table is built from.
#[test]
fn every_swept_run_records_what_it_found() {
    let (_dir, root, config) = workspace();
    tysserand_network(&config, &root, &mosna_pipeline::SilentProgress, &NoFigures).unwrap();

    let gmm = ClustererSweep {
        clusterer: "gmm".into(),
        axes: vec![interval("n_clusters", 2.0, 4.0, 3, true)],
    };
    let plan = sweep::plan(&[], &[gmm]);
    run_plan(&root, &config, &plan);

    let mut rows: Vec<sweep::Row> = plan
        .combinations
        .iter()
        .cloned()
        .map(sweep::Row::pending)
        .collect();

    for (index, row) in rows.iter_mut().enumerate() {
        let run = format!("1-1-{}", index + 1);
        let record: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("Niche_Analysis").join(&run).join("run.json"))
                .unwrap(),
        )
        .unwrap();
        row.run = Some(run);
        row.state = sweep::RunState::Done;
        row.absorb(&record);

        assert!(row.niches.is_some(), "a run recorded no niche count");
        assert!(row.largest.is_some(), "a run recorded no niche sizes");
    }

    // The niche count follows the cluster count that was asked for.
    let found: Vec<usize> = rows.iter().map(|r| r.niches.unwrap()).collect();
    assert_eq!(found, vec![2, 3, 4], "{found:?}");

    let csv = sweep::to_csv(&plan.columns(), &rows);
    assert_eq!(csv.lines().count(), 4, "three runs and a header");
    assert!(csv.lines().next().unwrap().contains("n_clusters"));
}
