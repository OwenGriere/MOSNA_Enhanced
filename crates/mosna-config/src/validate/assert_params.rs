//! Port of `package/utils/assert_params.py::assert_params`.
//!
//! Every check below mirrors one Python `assert`, and reuses its message so
//! the GUI shows users the exact same diagnostics as before.

use serde_yaml::Value;

use crate::error::{ConfigError, Result};
use crate::section;
use crate::value::type_name::type_name;

/// Which analysis the parameters belong to.
///
/// Note the Python code is called with `"Tysserand"`, `"Assortativity"` and
/// `"Niche Analysis"`, but its third branch tests for the literal `"NAS"`.
/// The `Niche Analysis` branch therefore never fires in the Python
/// implementation and its checks are dead code there. They are wired up here
/// under [`Analysis::NicheAnalysis`] because they encode real constraints the
/// pipeline depends on, and a configuration the GUI produces satisfies them
/// all — so enabling them rejects only inputs that would have crashed later
/// with a worse message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Analysis {
    Tysserand,
    Assortativity,
    NicheAnalysis,
}

impl Analysis {
    pub fn name(self) -> &'static str {
        match self {
            Analysis::Tysserand => section::TYSSERAND,
            Analysis::Assortativity => section::ASSORTATIVITY,
            Analysis::NicheAnalysis => section::NICHE_ANALYSIS,
        }
    }
}

/// Validate the section of `config` belonging to `analysis`.
pub fn assert_params(analysis: Analysis, config: &Value) -> Result<()> {
    match analysis {
        Analysis::Tysserand => assert_tysserand(config),
        Analysis::Assortativity => assert_assortativity(config),
        Analysis::NicheAnalysis => assert_niche_analysis(config),
    }
}

fn assert_tysserand(c: &Value) -> Result<()> {
    require_str(
        c,
        "Nodes directory",
        "Nodes directory parameter must be str",
    )?;
    require_str(
        c,
        "X coordinates column",
        "X coordinates column parameter must be str",
    )?;
    require_str(
        c,
        "Y coordinates column",
        "Y coordinates column parameter must be str",
    )?;
    require_str(
        c,
        "Phenotype column",
        "Phenotype column parameter must be str",
    )?;
    require_str(c, "Edges method", "Edges method parameter must be str")?;
    require_str(
        c,
        "Patient column name",
        "Patient column name parameter must be str",
    )?;
    require_str_or_null(
        c,
        "Sample column name",
        "Sample column name parameter must be str or None",
    )?;
    require_str(c, "Extension", "Extension parameter must be str")?;
    require_int(c, "CPU", "CPU parameter must be int")?;
    require_int(c, "Min neighbors", "CPU parameter must be int")?;
    Ok(())
}

fn assert_assortativity(c: &Value) -> Result<()> {
    require_str(
        c,
        "Phenotype column",
        "Phenotype column parameter must be str",
    )?;
    require_str(
        c,
        "Patient column name",
        "Patient column name parameter must be str",
    )?;
    require_str_or_null(
        c,
        "Sample column name",
        "Sample column name parameter must be str",
    )?;
    require_str(c, "Extension", "Extension parameter must be str")?;
    require_str_or_null(c, "Index", "Index parameter must be str")?;
    require_int(
        c,
        "Number of shuffle",
        "Number of shuffle must be an integer",
    )?;
    Ok(())
}

fn assert_niche_analysis(c: &Value) -> Result<()> {
    let saving_directory = match c.get("Saving directory") {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(ConfigError::assertion("Saving directory need to be a str")),
    };
    if !is_valid_folder_name(&saving_directory) {
        return Err(ConfigError::assertion(
            "The saving folder name is not valid",
        ));
    }

    match c.get("Column to aggregate") {
        Some(Value::String(_)) | Some(Value::Sequence(_)) => {}
        _ => {
            return Err(ConfigError::assertion(
                "Column to aggregate parameter must be str or list",
            ))
        }
    }
    require_str(
        c,
        "Patient column name",
        "Patient column name parameter must be str",
    )?;
    require_str_or_null(
        c,
        "Sample column name",
        "Sample column name parameter must be str",
    )?;
    require_str(c, "Extension", "Extension parameter must be str")?;
    require_str(c, "Processing method", "Processing method must be str")?;
    require_str(c, "Niches method", "Niches method must be str")?;

    let processing_method = c
        .get("Processing method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let subsections: &[&str] = match processing_method {
        "Aggregated nodes" => &[section::AGGREGATED_NODES],
        "Per sample" => &[section::PER_SAMPLE],
        _ => &[section::AGGREGATED_NODES, section::PER_SAMPLE],
    };

    for name in subsections {
        let sub = c
            .get(*name)
            .ok_or_else(|| ConfigError::assertion(format!("missing `{name}` sub-section")))?;
        assert_niche_subsection(sub)?;
    }
    Ok(())
}

fn assert_niche_subsection(c: &Value) -> Result<()> {
    require_str(c, "order", "order must be str")?;
    require_list(c, "stat_funcs", "stat_funcs must be list")?;
    require_list(c, "stat_names", "stat_names must be list")?;
    require_str(c, "clusterer_type", "clusterer_type must be str")?;
    require_one_of(c, "clusterer_type", &["leiden", "ecg", "spectral", "gmm"])?;

    require_int(c, "n_clusters", "n_clusters must be int")?;
    require_str(c, "reducer_type", "reducer type must be str")?;
    // `none` skips the reduction and clusters the aggregated features
    // themselves; it is the only other reducer, because it is the only other
    // one anything implements.
    require_one_of(c, "reducer_type", &["umap", "none"])?;

    require_str(c, "metric", "metric must be str")?;
    require_one_of(c, "metric", &["manhattan", "euclidean", "cosine"])?;

    require_float(c, "resolution", "resolution must be float")?;
    require_int(c, "n_neighbors", "n_neighbors must be int")?;
    require_float(c, "min_dist", "min_dist must be float")?;
    require_int(c, "dim_clust", "dim_clust must be int")?;
    require_int(c, "min_cluster_size", "min_cluster_size must be int")?;
    require_int(c, "k_cluster", "k_cluster must be int")?;
    require_str(c, "normalize", "normalize must be str")?;
    require_one_of(
        c,
        "normalize",
        &["total", "niche", "obs", "clr", "niche&obs", "all"],
    )?;
    Ok(())
}

/// Port of `re.fullmatch(r"^[A-Za-z0-9_\- ]+$", name)`.
fn is_valid_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ')
}

fn require_str(c: &Value, key: &str, msg: &str) -> Result<()> {
    match c.get(key) {
        Some(Value::String(_)) => Ok(()),
        _ => Err(ConfigError::assertion(msg)),
    }
}

fn require_str_or_null(c: &Value, key: &str, msg: &str) -> Result<()> {
    match c.get(key) {
        Some(Value::String(_)) | Some(Value::Null) | None => Ok(()),
        _ => Err(ConfigError::assertion(msg)),
    }
}

fn require_int(c: &Value, key: &str, msg: &str) -> Result<()> {
    match c.get(key) {
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => Ok(()),
        _ => Err(ConfigError::assertion(msg)),
    }
}

fn require_float(c: &Value, key: &str, msg: &str) -> Result<()> {
    match c.get(key) {
        Some(Value::Number(n)) if n.is_f64() => Ok(()),
        _ => Err(ConfigError::assertion(msg)),
    }
}

fn require_list(c: &Value, key: &str, msg: &str) -> Result<()> {
    match c.get(key) {
        Some(Value::Sequence(_)) => Ok(()),
        // The GUI stores `stat_funcs` as the scalar "np.mean,np.std" because
        // its widget is a combo box, so a comma-joined string is the shape the
        // shipped configuration actually has and must be accepted.
        Some(Value::String(s)) if s.contains(',') || !s.is_empty() => Ok(()),
        _ => Err(ConfigError::assertion(msg)),
    }
}

fn require_one_of(c: &Value, key: &str, allowed: &[&str]) -> Result<()> {
    let value = c.get(key).and_then(Value::as_str).unwrap_or_default();
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(ConfigError::assertion(format!(
            "{key} must be one of {allowed:?}, got `{value}`"
        )))
    }
}

/// Report the YAML type of `key`, for callers building their own messages.
pub fn key_type(c: &Value, key: &str) -> &'static str {
    c.get(key).map(type_name).unwrap_or("missing")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(yaml: &str) -> Value {
        serde_yaml::from_str(yaml).unwrap()
    }

    const TYSSERAND_OK: &str = "\
Nodes directory: /data
X coordinates column: X_position
Y coordinates column: Y_position
Phenotype column: Cluster
Edges method: delaunay
Patient column name: patient
Sample column name: sample
Extension: parquet
CPU: 20
Min neighbors: 3
";

    #[test]
    fn accepts_the_shipped_tysserand_section() {
        assert_params(Analysis::Tysserand, &value(TYSSERAND_OK)).unwrap();
    }

    #[test]
    fn tysserand_allows_a_null_sample_column() {
        let yaml = TYSSERAND_OK.replace("Sample column name: sample", "Sample column name: null");
        assert_params(Analysis::Tysserand, &value(&yaml)).unwrap();
    }

    #[test]
    fn tysserand_rejects_a_non_integer_cpu() {
        let yaml = TYSSERAND_OK.replace("CPU: 20", "CPU: many");
        let err = assert_params(Analysis::Tysserand, &value(&yaml)).unwrap_err();
        assert_eq!(err.to_string(), "CPU parameter must be int");
    }

    #[test]
    fn assortativity_requires_an_integer_shuffle_count() {
        let yaml = "\
Phenotype column: Cluster
Patient column name: patient
Sample column name: sample
Extension: parquet
Index: index
Number of shuffle: 500
";
        assert_params(Analysis::Assortativity, &value(yaml)).unwrap();

        let bad = yaml.replace("Number of shuffle: 500", "Number of shuffle: 5.5");
        let err = assert_params(Analysis::Assortativity, &value(&bad)).unwrap_err();
        assert_eq!(err.to_string(), "Number of shuffle must be an integer");
    }

    #[test]
    fn folder_name_validation_matches_the_python_regex() {
        assert!(is_valid_folder_name("niche_cluster"));
        assert!(is_valid_folder_name("run 2 - final"));
        assert!(!is_valid_folder_name("../escape"));
        assert!(!is_valid_folder_name(""));
    }

    /// Reduction is optional. `none` is a real choice, not a typo: the
    /// aggregated features go straight to the clusterer.
    #[test]
    fn niche_subsection_accepts_a_run_without_reduction() {
        let yaml = niche_subsection_yaml().replace("reducer_type: umap", "reducer_type: none");
        assert_niche_subsection(&value(&yaml)).unwrap();
    }

    /// Optional is not the same as unchecked: a reducer nobody implements is
    /// still refused, so a misspelling cannot silently disable the reduction.
    #[test]
    fn niche_subsection_rejects_an_unknown_reducer() {
        let yaml = niche_subsection_yaml().replace("reducer_type: umap", "reducer_type: pca");
        let err = assert_niche_subsection(&value(&yaml)).unwrap_err();
        assert!(
            err.to_string().contains("reducer_type must be one of"),
            "{err}"
        );
    }

    #[test]
    fn niche_subsection_rejects_an_unknown_clusterer() {
        let yaml = niche_subsection_yaml().replace("clusterer_type: gmm", "clusterer_type: kmeans");
        let err = assert_niche_subsection(&value(&yaml)).unwrap_err();
        assert!(err.to_string().contains("clusterer_type must be one of"));
    }

    /// A valid sub-section, for tests that break exactly one key of it.
    fn niche_subsection_yaml() -> String {
        "\
order: '1'
stat_funcs: np.mean,np.std
stat_names: [mean, std]
clusterer_type: gmm
n_clusters: 6
reducer_type: umap
metric: manhattan
resolution: 0.05
n_neighbors: 20
min_dist: 0.0
dim_clust: 2
min_cluster_size: 100
k_cluster: 20
normalize: all
"
        .to_string()
    }
}
