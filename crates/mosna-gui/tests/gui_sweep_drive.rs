//! A sweep driven through the interface's own methods, against the real
//! `mosna` binary.
//!
//! Every button of the sensitivity screen calls one of these: `open_sweep_gate`
//! for the way in, `start_sweep` for the gold button, `poll_run` for the frame
//! loop that drains a run's output and starts the next. Driving them in order
//! is the screen working, minus the pixels — which is the part no test was
//! going to cover anyway.
//!
//! Ignored by default: it starts dozens of sub-processes and takes longer than
//! a unit test should. Run it with
//! `cargo test -p mosna-gui --test gui_sweep_drive -- --ignored --nocapture`.

use std::path::{Path, PathBuf};

use mosna_gui::app::{MosnaApp, Screen};
use mosna_gui::model::runner::Step;
use mosna_gui::model::sweep::{Sampling, SweepForm};

/// A light cohort: three samples of a hundred cells, four phenotypes.
fn cohort(raw: &Path, samples: usize, cells: usize) {
    std::fs::create_dir_all(raw).unwrap();
    let side = (cells as f64).sqrt().ceil() as usize;

    for sample in 1..=samples {
        let mut xs = Vec::with_capacity(cells);
        let mut ys = Vec::with_capacity(cells);
        let mut labels = Vec::with_capacity(cells);
        for i in 0..cells {
            // Jittered, so the triangulation is not degenerate, and seeded off
            // the sample so the three are not identical.
            xs.push((i / side) as f64 + ((i * 7 + sample) % 5) as f64 * 0.03);
            ys.push((i % side) as f64 + ((i * 11 + sample) % 5) as f64 * 0.03);
            labels.push(["A", "B", "C", "D"][(i + sample) % 4]);
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
  CPU: 4
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
    reducer_type: umap
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
    reducer_type: umap
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
";

/// Drive the frame loop until nothing is in flight.
fn pump(app: &mut MosnaApp, what: &str) {
    let started = std::time::Instant::now();
    loop {
        app.poll_run();
        if app.run.is_none() && app.sweep.as_ref().is_none_or(|s| !s.is_running()) {
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(900),
            "{what} did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Tick these values in a whole-number menu, as the drop-down would.
fn tick(form: &mut SweepForm, key: &str, from: &str, to: &str, wanted: &[&str]) {
    let axis = form
        .shared
        .iter_mut()
        .chain(form.clusterers.iter_mut().flat_map(|c| c.axes.iter_mut()))
        .find(|a| a.key == key)
        .unwrap_or_else(|| panic!("`{key}` is not on the screen"));
    assert_eq!(Sampling::of(key), Sampling::Integer, "`{key}`");
    axis.from = from.into();
    axis.to = to.into();
    axis.refresh_candidates();
    for (name, picked) in &mut axis.choices {
        *picked = wanted.contains(&name.as_str());
    }
}

#[test]
#[ignore = "starts dozens of sub-processes; run with --ignored"]
fn a_sweep_of_many_runs_driven_through_the_interface() {
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/release/mosna")
        .canonicalize()
        .expect("build the release binary first: cargo build --release -p mosna-cli");
    std::env::set_var("MOSNA_BIN", &binary);

    // The renderer, when this checkout has one. Without it the figures are not
    // drawn — which the analyses themselves survive, by design — so the test
    // says which of the two it is measuring rather than quietly checking less.
    // Not canonicalised: a virtual environment's `bin/python` is a symlink to
    // the interpreter it was built from, and resolving it hands back the base
    // interpreter — which is the one *without* the renderer installed.
    let venv = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python");
    let renderer = venv.is_file().then_some(venv);
    match &renderer {
        Some(python) => std::env::set_var("MOSNA_PYTHON", python),
        None => println!("no renderer in this checkout: the figures are not checked"),
    }

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    cohort(&root.join("raw"), 3, 100);

    let config_path = root.join("configuration.yaml");
    std::fs::write(&config_path, CONFIG).unwrap();

    let mut app = MosnaApp::new(config_path);
    app.browser.working_dir = Some(root.clone());
    app.needs_working_dir = false;

    // Step 1, as the action bar starts it.
    app.start(Step::Tysserand);
    pump(&mut app, "step 1");
    assert!(!app.last_run_failed, "step 1 failed: {:?}", app.status);
    println!("step 1: {}", app.status);

    // The way into the screen: save, check, confirm.
    app.open_sweep_gate();
    let gate = app.sweep_gate.as_ref().expect("a confirmation dialog");
    println!("\nthe settings every run will share:");
    for (name, value) in &gate.summary {
        println!("  {name:<28} {}", value.caption());
    }
    assert!(gate.is_ready(), "{:?}", gate.problems);
    app.enter_sweep_screen();
    assert_eq!(app.screen, Screen::Sensitivity);

    // A grid: two neighbourhood orders, three projections, and each of two
    // clusterers varying what only it reads.
    let form = &mut app.sweep_form;
    for (_, picked) in &mut form
        .shared
        .iter_mut()
        .find(|a| a.key == "order")
        .unwrap()
        .choices
    {
        *picked = true;
    }
    tick(form, "dim_clust", "2", "4", &["2", "3", "4"]);

    for clusterer in &mut form.clusterers {
        clusterer.selected = clusterer.clusterer != "spectral";
    }
    tick(form, "n_clusters", "2", "5", &["2", "3", "4", "5"]);
    let resolution = form
        .clusterers
        .iter_mut()
        .find(|c| c.clusterer == "leiden")
        .unwrap()
        .axes
        .iter_mut()
        .find(|a| a.key == "resolution")
        .unwrap();
    resolution.from = "0.005".into();
    resolution.to = "0.5".into();
    resolution.count = "5".into();

    let plan = app.sweep_form.plan();
    println!(
        "\ngrid: {} runs ({} asked for), {} aggregation(s), {} projection(s)",
        plan.len(),
        plan.requested,
        plan.aggregations(),
        plan.reductions()
    );
    assert!(plan.len() >= 40, "only {} runs", plan.len());

    // The gold button.
    let started = std::time::Instant::now();
    app.start_sweep();
    assert!(
        app.sweep.is_some(),
        "the sweep did not start: {:?}",
        app.notice
    );
    pump(&mut app, "the sweep");
    let elapsed = started.elapsed();

    let sweep = app.sweep.as_ref().unwrap();
    println!(
        "\n{}\n{} run(s) in {:.1} s — {:.2} s each",
        sweep.caption(),
        sweep.total(),
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / sweep.total() as f64
    );
    if let Some(reason) = &sweep.failure {
        println!("first failure: {reason}");
    }

    println!("\nrun            clusterer  niches  largest  floor");
    for row in sweep.rows.iter().take(12) {
        println!(
            "{:<14} {:<10} {:<7} {:<8} {}",
            row.run.clone().unwrap_or_else(|| "—".into()),
            row.combination.clusterer,
            row.niches.map(|n| n.to_string()).unwrap_or_default(),
            row.largest.map(|n| n.to_string()).unwrap_or_default(),
            row.graph_components
                .map(|n| n.to_string())
                .unwrap_or_default(),
        );
    }

    assert_eq!(sweep.failed(), 0, "{} run(s) failed", sweep.failed());
    assert_eq!(sweep.percent(), 100);
    assert!(sweep.rows.iter().all(|row| row.niches.is_some()));

    // Where everything landed.
    let sensitivity = root.join("Niche_Analysis/sensitivity_analysis");
    let runs: Vec<String> = std::fs::read_dir(&sensitivity)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(runs.len(), sweep.total(), "a run left no directory");
    assert!(sensitivity.join("sensitivity.csv").is_file());

    println!("\n--- what one run directory holds ---");
    for entry in std::fs::read_dir(sensitivity.join(&runs[0]))
        .unwrap()
        .flatten()
    {
        println!("  {}", entry.file_name().to_string_lossy());
    }

    // PNG only, as a sweep asks for — and PNGs at all, which an assertion on
    // the absence of HTML alone would have been satisfied by drawing nothing.
    let extensions = |what: &str| {
        std::fs::read_dir(sensitivity.join(&runs[0]))
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == what))
            .count()
    };
    assert_eq!(extensions("html"), 0, "a sweep drew interactive figures");
    if renderer.is_some() {
        assert!(extensions("png") > 0, "a sweep drew no figures at all");
    }

    // One aggregation per order, shared by every run under it.
    let aggregations = std::fs::read_dir(root.join("temp/intermediate_files"))
        .unwrap()
        .flatten()
        .count();
    println!(
        "\n{aggregations} aggregation(s) on disk for {} runs",
        sweep.total()
    );
    assert_eq!(aggregations, plan.aggregations());

    // The comparison is one more sub-process, started once the last run lands.
    pump(&mut app, "the comparison");
    println!("\nstatus after comparing: {}", app.status);

    println!("\n--- comparison figures ---");
    let mut drawn = 0;
    for stem in [
        "Sensitivity_Agreement",
        "Sensitivity_Agreement_Matrix",
        "Sensitivity_Niche_Stability",
    ] {
        let png = sensitivity.join(format!("{stem}.png"));
        let html = sensitivity.join(format!("{stem}.html"));
        println!(
            "  {stem:<32} png={:<7} html={}",
            png.is_file(),
            html.is_file()
        );
        drawn += usize::from(png.is_file() && html.is_file());
    }
    if renderer.is_some() {
        assert_eq!(drawn, 3, "the comparison figures were not drawn");
    }

    println!("\n--- sensitivity.csv ---");
    let csv = std::fs::read_to_string(sensitivity.join("sensitivity.csv")).unwrap();
    for line in csv.lines().take(8) {
        println!("{line}");
    }
    println!("... {} lines", csv.lines().count());
}
