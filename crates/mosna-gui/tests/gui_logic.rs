//! Tests of the interface's logic, written before the implementation.
//!
//! The rendering itself is not unit-testable in a useful way, but everything
//! *behind* it is: which widget a configuration key gets, how a widget's value
//! is read back, which tab and group a key lands in, how a log line is
//! classified, what a progress line means, and which images belong to which
//! patient. That logic is the part that has to match `GUI_MOSNA.py` exactly,
//! and it is all here.

use std::path::Path;

use mosna_config::RawConfig;
use mosna_gui::model::browser::{BrowserState, SampleRow};
use mosna_gui::model::field::{Field, FieldKind};
use mosna_gui::model::form::Form;
use mosna_gui::model::log::{classify, LogKind};
use mosna_gui::model::runner::{format_duration, parse_output_line, OutputLine, Step};
use mosna_gui::model::viewer::collect_analysis_images;
use serde_yaml::Value;

const CONFIG: &str = "\
Tysserand:
  Nodes directory: /data
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
  X coordinates column: X_position
  Y coordinates column: Y_position
  Phenotype column: Cluster
  Edges method: delaunay
  Min neighbors: 3
  CPU: 20
Assortativity:
  Network directory: Default
  Phenotype column: null
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
  Index: index
  Number of shuffle: 500
  Randomization diagnostic: false
Niche Analysis:
  Network directory: Default
  Saving directory: niche_cluster
  Extension: parquet
  Patient column name: patient
  Sample column name: sample
  Processing method: Aggregated nodes
  Niches method: NAS
  Phenotype column: null
  Column to aggregate: null
  Plot Network: true
  X coordinates column for niches: null
  Y coordinates column for niches: null
  CPU: 20
  Aggregated nodes:
    reducer_type: umap
    dim_clust: 2
    n_neighbors: 20
    metric: manhattan
    min_dist: 0.0
    clusterer_type: gmm
    k_cluster: 20
    n_clusters: 6
    resolution: 0.05
    min_cluster_size: 100
    normalize: all
    order: '1'
    stat_funcs: np.mean,np.std
    stat_names: [mean, std]
";

fn config() -> RawConfig {
    RawConfig::from_yaml_str(CONFIG).unwrap()
}

// ---------------------------------------------------------------------------
// Which widget each key gets — port of ParametersPanel._get_widget
// ---------------------------------------------------------------------------

/// The five coordinate and phenotype keys become drop-downs listing the columns
/// of the selected nodes file, so the user cannot mistype a column name.
#[test]
fn column_keys_become_column_pickers() {
    for key in [
        "X coordinates column",
        "Y coordinates column",
        "Phenotype column",
        "X coordinates column for niches",
        "Y coordinates column for niches",
    ] {
        let field = Field::for_key(key, &Value::Null);
        assert!(
            matches!(field.kind, FieldKind::ColumnPicker { .. }),
            "`{key}` should pick a column, got {:?}",
            field.kind
        );
    }
}

#[test]
fn column_to_aggregate_allows_several_columns() {
    let field = Field::for_key("Column to aggregate", &Value::Null);
    assert!(matches!(field.kind, FieldKind::MultiColumnPicker { .. }));
}

#[test]
fn booleans_become_a_two_item_choice() {
    let field = Field::for_key("Plot Network", &Value::Bool(true));
    match &field.kind {
        FieldKind::Choice { options, .. } => assert_eq!(options, &["True", "False"]),
        other => panic!("expected a choice, got {other:?}"),
    }
}

/// The fixed option lists are the ones the Python hard-codes, and they are what
/// stops an unsupported algorithm being selected.
#[test]
fn keys_with_fixed_options_become_a_choice() {
    let field = Field::for_key("clusterer_type", &Value::String("gmm".into()));
    match &field.kind {
        FieldKind::Choice { options, selected } => {
            assert_eq!(
                options,
                &["leiden", "ecg", "spectral", "gmm", "hdbscan"],
                "the clusterer list must match the Python"
            );
            assert_eq!(options[*selected], "gmm");
        }
        other => panic!("expected a choice, got {other:?}"),
    }

    let metric = Field::for_key("metric", &Value::String("manhattan".into()));
    match &metric.kind {
        FieldKind::Choice { options, .. } => {
            assert_eq!(options, &["manhattan", "euclidean", "cosine"])
        }
        other => panic!("expected a choice, got {other:?}"),
    }
}

#[test]
fn the_index_key_gets_a_mode_and_a_column() {
    let field = Field::for_key("Index", &Value::String("index".into()));
    assert!(matches!(field.kind, FieldKind::IndexPicker { .. }));
}

#[test]
fn the_saving_directory_gets_a_browse_button() {
    let field = Field::for_key("Saving directory", &Value::String("niche_cluster".into()));
    assert!(matches!(field.kind, FieldKind::DirectoryPath { .. }));
}

#[test]
fn anything_else_is_a_text_box() {
    let field = Field::for_key("Number of shuffle", &Value::Number(500.into()));
    assert!(matches!(field.kind, FieldKind::Text { .. }));
}

// ---------------------------------------------------------------------------
// Reading a widget back — port of ParametersPanel.parse_value
// ---------------------------------------------------------------------------

/// `order` is returned as text without any coercion, because `assert_params`
/// requires it to be a string: turning `'1'` into the integer 1 would make the
/// configuration invalid.
#[test]
fn the_order_key_stays_a_string() {
    let field = Field::for_key("order", &Value::String("1".into()));
    assert_eq!(field.value(), Value::String("1".into()));
}

#[test]
fn empty_and_null_text_reads_back_as_null() {
    for text in ["", "none", "None", "null", "NULL"] {
        let mut field = Field::for_key("Some key", &Value::Null);
        field.set_text(text);
        assert_eq!(field.value(), Value::Null, "`{text}` should read as null");
    }
}

#[test]
fn boolean_text_reads_back_as_a_boolean() {
    let mut field = Field::for_key("Some key", &Value::Null);
    field.set_text("True");
    assert_eq!(field.value(), Value::Bool(true));
    field.set_text("false");
    assert_eq!(field.value(), Value::Bool(false));
}

#[test]
fn numbers_read_back_as_numbers() {
    let mut field = Field::for_key("CPU", &Value::Null);
    field.set_text("20");
    assert_eq!(field.value(), Value::Number(20.into()));

    field.set_text("0.05");
    assert_eq!(field.value().as_f64(), Some(0.05));
}

/// An integer must not become a float: `assert_params` checks `isinstance(v,
/// int)` for `CPU`, `n_clusters` and friends, and a float would be rejected.
#[test]
fn an_integer_does_not_become_a_float() {
    let mut field = Field::for_key("n_clusters", &Value::Null);
    field.set_text("6");
    let value = field.value();
    assert!(value.is_i64(), "expected an integer, got {value:?}");
}

#[test]
fn a_bracketed_value_reads_back_as_a_list() {
    let mut field = Field::for_key("stat_names", &Value::Null);
    field.set_text("[mean, std]");
    match field.value() {
        Value::Sequence(items) => assert_eq!(items.len(), 2),
        other => panic!("expected a list, got {other:?}"),
    }
}

#[test]
fn an_unselected_picker_reads_back_as_null() {
    let field = Field::for_key("Phenotype column", &Value::Null);
    assert_eq!(field.value(), Value::Null);
}

#[test]
fn a_multi_column_picker_collapses_a_single_choice() {
    let mut field = Field::for_key("Column to aggregate", &Value::Null);
    field.set_available_columns(&["Cluster".into(), "Type".into()]);

    field.set_selected_columns(&["Cluster".into()]);
    assert_eq!(field.value(), Value::String("Cluster".into()));

    field.set_selected_columns(&["Cluster".into(), "Type".into()]);
    assert!(matches!(field.value(), Value::Sequence(items) if items.len() == 2));

    field.set_selected_columns(&[]);
    assert_eq!(field.value(), Value::Null);
}

/// `Index: index` is the sentinel for "use the positional index"; only the
/// `Custom` mode reports a column name.
#[test]
fn the_index_picker_reports_its_sentinel() {
    let mut field = Field::for_key("Index", &Value::String("index".into()));
    assert_eq!(field.value(), Value::String("index".into()));

    field.set_available_columns(&["cell_id".into()]);
    field.set_custom_index("cell_id");
    assert_eq!(field.value(), Value::String("cell_id".into()));
}

// ---------------------------------------------------------------------------
// The form layout — port of ParametersPanel._add_section_tab and friends
// ---------------------------------------------------------------------------

/// The three analyses come first and in this order, whatever the file order.
#[test]
fn sections_are_laid_out_in_workflow_order() {
    let form = Form::from_config(&config());
    let names: Vec<&str> = form.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Tysserand", "Assortativity", "Niche Analysis"]);
}

/// The keys the Browser panel owns are not repeated in the Parameters panel;
/// the Python filters them out with `BROWSER_KEYS`.
#[test]
fn browser_keys_are_not_duplicated_in_the_parameters() {
    let form = Form::from_config(&config());
    for section in &form.sections {
        for tab in &section.tabs {
            for group in &tab.groups {
                for field in &group.fields {
                    assert!(
                        !matches!(
                            field.key.as_str(),
                            "Nodes directory"
                                | "Network directory"
                                | "Patient column name"
                                | "Sample column name"
                                | "Extension"
                        ),
                        "`{}` belongs to the Browser panel",
                        field.key
                    );
                }
            }
        }
    }
}

/// A sub-section of the configuration becomes its own inner tab.
#[test]
fn niche_sub_sections_become_inner_tabs() {
    let form = Form::from_config(&config());
    let niche = form
        .sections
        .iter()
        .find(|s| s.name == "Niche Analysis")
        .unwrap();

    let tabs: Vec<&str> = niche.tabs.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(tabs, vec!["General", "Aggregated nodes"]);
}

/// Within a niche sub-section the settings are grouped by what they control.
#[test]
fn niche_settings_are_grouped_by_purpose() {
    let form = Form::from_config(&config());
    let aggregated = form
        .sections
        .iter()
        .find(|s| s.name == "Niche Analysis")
        .unwrap()
        .tabs
        .iter()
        .find(|t| t.name == "Aggregated nodes")
        .unwrap();

    let groups: Vec<&str> = aggregated.groups.iter().map(|g| g.title.as_str()).collect();
    assert!(groups.contains(&"Reduction"), "{groups:?}");
    assert!(groups.contains(&"Clustering"), "{groups:?}");
    assert!(groups.contains(&"Niche Normalisation"), "{groups:?}");

    let reduction = aggregated
        .groups
        .iter()
        .find(|g| g.title == "Reduction")
        .unwrap();
    let keys: Vec<&str> = reduction.fields.iter().map(|f| f.key.as_str()).collect();
    for expected in [
        "reducer_type",
        "dim_clust",
        "n_neighbors",
        "metric",
        "min_dist",
    ] {
        assert!(
            keys.contains(&expected),
            "Reduction is missing `{expected}`"
        );
    }
}

#[test]
fn the_niche_general_tab_separates_the_replot_settings() {
    let form = Form::from_config(&config());
    let general = form
        .sections
        .iter()
        .find(|s| s.name == "Niche Analysis")
        .unwrap()
        .tabs
        .iter()
        .find(|t| t.name == "General")
        .unwrap();

    let groups: Vec<&str> = general.groups.iter().map(|g| g.title.as_str()).collect();
    assert!(groups.contains(&"Niche General Parameters"), "{groups:?}");
    assert!(
        groups.contains(&"Replot Network with Niche Labels"),
        "{groups:?}"
    );
}

/// A parameter that the chosen algorithm ignores is disabled, so the user is
/// not led into setting something with no effect.
#[test]
fn clustering_parameters_follow_the_chosen_algorithm() {
    let mut form = Form::from_config(&config());

    form.set_clusterer("Niche Analysis", "Aggregated nodes", "leiden");
    assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "resolution"));
    assert!(!form.is_enabled("Niche Analysis", "Aggregated nodes", "n_clusters"));

    form.set_clusterer("Niche Analysis", "Aggregated nodes", "gmm");
    assert!(!form.is_enabled("Niche Analysis", "Aggregated nodes", "resolution"));
    assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "n_clusters"));

    form.set_clusterer("Niche Analysis", "Aggregated nodes", "hdbscan");
    assert!(form.is_enabled("Niche Analysis", "Aggregated nodes", "min_cluster_size"));
    assert!(!form.is_enabled("Niche Analysis", "Aggregated nodes", "n_clusters"));
}

#[test]
fn parameters_carry_their_explanatory_tooltip() {
    let form = Form::from_config(&config());
    let field = form
        .field("Niche Analysis", "Aggregated nodes", "resolution")
        .unwrap();
    let tooltip = field.tooltip.unwrap_or_default();
    assert!(tooltip.contains("Leiden"), "got `{tooltip}`");
}

/// Editing the form and writing it back must preserve everything else in the
/// document, which is what the Save button relies on.
#[test]
fn the_form_writes_back_into_the_configuration() {
    let original = config();
    let mut form = Form::from_config(&original);
    form.set_text("Tysserand", "General", "Min neighbors", "5");

    let mut updated = original.clone();
    form.apply_to(&mut updated);

    assert_eq!(
        updated.get("Tysserand", "Min neighbors"),
        Some(&Value::Number(5.into()))
    );
    // Untouched keys survive.
    assert_eq!(
        updated.get("Tysserand", "Edges method"),
        Some(&Value::String("delaunay".into()))
    );
    assert_eq!(
        updated.get("Niche Analysis", "Saving directory"),
        Some(&Value::String("niche_cluster".into()))
    );
}

// ---------------------------------------------------------------------------
// The Browser panel
// ---------------------------------------------------------------------------

#[test]
fn the_browser_reads_its_values_from_the_configuration() {
    let browser = BrowserState::from_config(&config());
    assert_eq!(browser.nodes_directory, "/data");
    assert_eq!(browser.patient_column, "patient");
    assert_eq!(browser.sample_column, "sample");
    assert_eq!(browser.extension, "parquet");
    assert!(browser.network_directory_is_default);
}

/// Discovering the nodes files fills the table the way the Python does: patient,
/// sample, the nodes file name, and the matching edges file when it exists.
#[test]
fn refreshing_lists_the_discovered_samples() {
    let dir = tempfile::tempdir().unwrap();
    for (patient, sample) in [("1", "1"), ("2", "1")] {
        std::fs::write(
            dir.path()
                .join(format!("nodes_patient-{patient}_sample-{sample}.parquet")),
            b"",
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("edges_patient-1_sample-1.parquet"), b"").unwrap();

    let mut browser = BrowserState::from_config(&config());
    browser.nodes_directory = dir.path().to_string_lossy().into_owned();

    let rows = browser.discover_nodes().unwrap();
    assert_eq!(rows.len(), 2);

    let first: &SampleRow = &rows[0];
    assert_eq!(first.patient, "1");
    assert_eq!(first.sample.as_deref(), Some("1"));
    assert_eq!(first.nodes_file, "nodes_patient-1_sample-1.parquet");
    assert_eq!(
        first.edges_file.as_deref(),
        Some("edges_patient-1_sample-1.parquet")
    );
    // The second sample has no edges file yet.
    assert_eq!(rows[1].edges_file, None);
}

#[test]
fn an_unreadable_directory_is_reported_rather_than_panicking() {
    let mut browser = BrowserState::from_config(&config());
    browser.nodes_directory = "/nonexistent/mosna".into();
    assert!(browser.discover_nodes().is_err());
}

/// The browser's values are pushed into all three sections, exactly as
/// `_apply_browser_values_to_config` does.
#[test]
fn browser_values_are_written_into_every_section() {
    let mut configuration = config();
    let mut browser = BrowserState::from_config(&configuration);
    browser.patient_column = "case".into();
    browser.sample_column = "slide".into();
    browser.extension = "csv".into();
    browser.apply_to(&mut configuration);

    for section in ["Tysserand", "Assortativity", "Niche Analysis"] {
        assert_eq!(
            configuration.get(section, "Patient column name"),
            Some(&Value::String("case".into())),
            "section `{section}`"
        );
        assert_eq!(
            configuration.get(section, "Extension"),
            Some(&Value::String("csv".into()))
        );
    }
}

/// Clearing the sample column means a single-level dataset, which the discovery
/// pattern has to reflect.
#[test]
fn an_empty_sample_column_means_a_single_level_dataset() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("nodes_patient-7.parquet"), b"").unwrap();
    std::fs::write(dir.path().join("nodes_patient-8_sample-1.parquet"), b"").unwrap();

    let mut browser = BrowserState::from_config(&config());
    browser.nodes_directory = dir.path().to_string_lossy().into_owned();
    browser.sample_column = String::new();

    let rows = browser.discover_nodes().unwrap();
    assert_eq!(rows.len(), 1, "the two-level file must not match");
    assert_eq!(rows[0].patient, "7");
    assert_eq!(rows[0].sample, None);
}

// ---------------------------------------------------------------------------
// The log panel
// ---------------------------------------------------------------------------

#[test]
fn log_lines_are_classified_by_severity() {
    assert_eq!(classify("[ERROR] boom"), LogKind::Error);
    assert_eq!(classify("❌ boom"), LogKind::Error);
    assert_eq!(classify("[WARN] careful"), LogKind::Warning);
    assert_eq!(classify("[OK] done"), LogKind::Success);
    assert_eq!(classify("✅ done"), LogKind::Success);
    assert_eq!(classify("[INFO] hello"), LogKind::Info);
    assert_eq!(classify("[QT_INFO] hello"), LogKind::Info);
    assert_eq!(
        classify("[QT_PROGRESS] current=1 total=2 desc=x"),
        LogKind::Progress
    );
    assert_eq!(classify("anything else"), LogKind::Plain);
}

/// Severity is decided in the Python's order: a line carrying both an error and
/// an info marker is an error.
#[test]
fn severity_wins_over_information() {
    assert_eq!(classify("[INFO] [ERROR] both"), LogKind::Error);
}

// ---------------------------------------------------------------------------
// Driving a run
// ---------------------------------------------------------------------------

/// The interface parses the analysis process's stdout to move its progress bar.
#[test]
fn progress_lines_are_parsed() {
    match parse_output_line("[QT_PROGRESS] current=3 total=12 desc=[PROCESS] Working") {
        OutputLine::Progress {
            current,
            total,
            description,
        } => {
            assert_eq!(current, 3);
            assert_eq!(total, 12);
            assert_eq!(description, "[PROCESS] Working");
        }
        other => panic!("expected progress, got {other:?}"),
    }
}

#[test]
fn info_lines_become_the_status_message() {
    match parse_output_line("[QT_INFO] Parameters are read correctly") {
        OutputLine::Info(message) => assert_eq!(message, "Parameters are read correctly"),
        other => panic!("expected info, got {other:?}"),
    }
}

#[test]
fn other_lines_are_plain_output() {
    assert!(matches!(
        parse_output_line("Compiling something"),
        OutputLine::Plain
    ));
    // A malformed progress line must not be mistaken for a real one.
    assert!(matches!(
        parse_output_line("[QT_PROGRESS] nonsense"),
        OutputLine::Plain
    ));
}

/// Each button maps to one sub-command of the `mosna` binary, with the same
/// flags the Python passed to its modules.
#[test]
fn each_step_builds_its_command_line() {
    let config_path = Path::new("/cfg/configuration.yaml");
    let working_dir = Path::new("/work");

    let expected = [
        (Step::Tysserand, "tysserand-network"),
        (Step::Assortativity, "assortativity"),
        (Step::NicheAnalysis, "niche-analysis"),
        (Step::ClearTemporary, "clear-temporary"),
    ];

    for (step, sub_command) in expected {
        let arguments = step.arguments(config_path, working_dir);
        assert_eq!(arguments[0], sub_command, "wrong sub-command for {step:?}");
        assert!(
            arguments.contains(&"--working_dir".to_string()),
            "{step:?} must pass --working_dir"
        );
        if step == Step::ClearTemporary {
            assert!(
                !arguments.contains(&"--file".to_string()),
                "clearing takes no configuration"
            );
        } else {
            assert!(arguments.contains(&"--file".to_string()));
        }
    }
}

#[test]
fn every_step_has_a_button_label_naming_its_position() {
    assert!(Step::Tysserand.label().contains("Step 1"));
    assert!(Step::Assortativity.label().contains("Step 2"));
    assert!(Step::NicheAnalysis.label().contains("Step 3"));
    assert_eq!(Step::ClearTemporary.label(), "Clear Temp Files");
}

#[test]
fn durations_are_formatted_the_way_the_python_does() {
    assert_eq!(format_duration(2.5), "2.50 s");
    assert_eq!(format_duration(90.0), "1 min 30 s");
    assert_eq!(format_duration(3725.0), "1 h 2 min 5 s");
}

// ---------------------------------------------------------------------------
// The image viewer
// ---------------------------------------------------------------------------

/// Figures are collected from the directories the analyses write into, and
/// grouped by patient so the viewer's drop-down can switch between them.
#[test]
fn images_are_collected_and_grouped_by_patient() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    std::fs::create_dir_all(root.join("Tysserand_Network")).unwrap();
    std::fs::write(root.join("Tysserand_Network/net_1-1.png"), b"").unwrap();
    std::fs::write(root.join("Tysserand_Network/net_2-1.png"), b"").unwrap();

    std::fs::create_dir_all(root.join("Assortativity/assort_files")).unwrap();
    std::fs::write(root.join("Assortativity/abundance.png"), b"").unwrap();
    std::fs::write(
        root.join("Assortativity/assort_files/heatmap_zscore_1-1.png"),
        b"",
    )
    .unwrap();

    let images = collect_analysis_images(root);

    assert_eq!(images.tysserand.patients.len(), 2);
    assert!(images.tysserand.patients.contains_key("1"));
    assert!(images.tysserand.patients.contains_key("2"));

    assert_eq!(images.assortativity.global.len(), 1);
    assert_eq!(images.assortativity.patients["1"].len(), 1);
}

/// The niche figures live under `Niche_Analysis`, which is the directory step 3
/// actually writes to.
#[test]
fn niche_images_are_found_where_step_three_writes_them() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let save_dir = root.join("Niche_Analysis/Aggregation/niche_cluster");
    std::fs::create_dir_all(&save_dir).unwrap();
    std::fs::write(save_dir.join("Niches_Histogram.png"), b"").unwrap();

    let images = collect_analysis_images(root);
    assert_eq!(
        images.niches.global.len(),
        1,
        "the niche figures were not found"
    );
}

#[test]
fn a_working_directory_without_results_yields_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let images = collect_analysis_images(dir.path());
    assert!(images.tysserand.patients.is_empty());
    assert!(images.assortativity.global.is_empty());
    assert!(images.niches.global.is_empty());
}
