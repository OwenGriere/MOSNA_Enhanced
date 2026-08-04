//! Checks the shipped `CONFIG/configuration.yaml` loads, validates and
//! round-trips through the Rust implementation without drifting.

use std::path::PathBuf;

use mosna_config::validate::assert_params::{assert_params, Analysis};
use mosna_config::{get_config, section, TysserandConfig};

fn config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("CONFIG/configuration.yaml")
}

/// The Tysserand section of the shipped file is fully populated, so both the
/// validator and the typed view must accept it as-is.
#[test]
fn shipped_tysserand_section_is_accepted() {
    let path = config_path();
    if !path.exists() {
        eprintln!("skipping: {} not found", path.display());
        return;
    }
    let cfg = get_config(&path).expect("configuration.yaml must parse");

    assert_params(
        Analysis::Tysserand,
        cfg.section(section::TYSSERAND).unwrap(),
    )
    .unwrap();

    let tys = TysserandConfig::from_raw(&cfg).expect("Tysserand section must be typed");
    assert_eq!(tys.patient_column, "patient");
    assert_eq!(tys.extension, "parquet");
    assert!(tys.cpu >= 1);
}

/// The shipped file leaves `Phenotype column` and `Column to aggregate` unset
/// for steps 2 and 3 — the user picks them in the GUI once a nodes file has
/// been read and its columns are known. Python rejects that state with
/// `assert isinstance(config["Phenotype column"], str)`; this asserts the Rust
/// port refuses it the same way and with the same message, rather than
/// silently accepting a half-configured run.
#[test]
fn unset_columns_are_rejected_exactly_like_python() {
    let path = config_path();
    if !path.exists() {
        return;
    }
    let cfg = get_config(&path).unwrap();

    let err = assert_params(
        Analysis::Assortativity,
        cfg.section(section::ASSORTATIVITY).unwrap(),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Phenotype column parameter must be str");

    let err = assert_params(
        Analysis::NicheAnalysis,
        cfg.section(section::NICHE_ANALYSIS).unwrap(),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Column to aggregate parameter must be str or list"
    );
}

/// Loading and re-emitting the configuration must leave the file untouched,
/// so that saving from the GUI produces a clean diff.
#[test]
fn shipped_configuration_round_trips_byte_for_byte() {
    let path = config_path();
    if !path.exists() {
        return;
    }
    let original = std::fs::read_to_string(&path).unwrap();
    let cfg = get_config(&path).unwrap();
    let rendered = cfg.to_yaml_string().unwrap();
    assert_eq!(
        rendered, original,
        "re-emitting the configuration must not change it"
    );
}

/// Once the user fills in the columns the GUI would populate, every section
/// validates and every typed view builds.
#[test]
fn fully_populated_configuration_is_accepted() {
    let path = config_path();
    if !path.exists() {
        return;
    }
    let mut cfg = get_config(&path).unwrap();
    cfg.set(
        section::ASSORTATIVITY,
        "Phenotype column",
        serde_yaml::Value::String("Cluster".into()),
    );
    cfg.set(
        section::NICHE_ANALYSIS,
        "Phenotype column",
        serde_yaml::Value::String("Cluster".into()),
    );
    cfg.set(
        section::NICHE_ANALYSIS,
        "Column to aggregate",
        serde_yaml::Value::String("Cluster".into()),
    );

    for analysis in [
        Analysis::Tysserand,
        Analysis::Assortativity,
        Analysis::NicheAnalysis,
    ] {
        assert_params(analysis, cfg.section(analysis.name()).unwrap())
            .unwrap_or_else(|e| panic!("{} section rejected: {e}", analysis.name()));
    }

    mosna_config::TysserandConfig::from_raw(&cfg).unwrap();
    mosna_config::AssortativityConfig::from_raw(&cfg).unwrap();
    let niche = mosna_config::NicheAnalysisConfig::from_raw(&cfg).unwrap();
    assert!(
        niche.make_onehot(),
        "a single column must be one-hot encoded"
    );
    assert_eq!(niche.aggregated.n_clusters, 6);
    assert_eq!(niche.aggregated.dim_clust, 2);
}
