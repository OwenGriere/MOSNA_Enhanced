//! Port of `package/utils/assert_params.py::assert_params`.
//!
//! Every check below mirrors one Python `assert`, and reuses its message so
//! the GUI shows users the exact same diagnostics as before.

use serde_yaml::Value;

use crate::error::{ConfigError, Result};
use crate::model::niche_params::{
    split_stat_list, IMPLEMENTED_CLUSTERERS, IMPLEMENTED_REDUCERS, IMPLEMENTED_STAT_FUNCS,
};
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
    // `order` is the one integer of this section the shipped configuration
    // spells as a string (`order: '1'`), because the interface's drop-down
    // stores its choices as text. Both spellings are accepted: demanding the
    // string rejected every configuration built by a program, and demanding the
    // integer would reject every configuration the interface has ever written.
    require_number_or_numeric_str(c, "order", "order must be an integer")?;
    require_stat_funcs(c)?;
    require_list(c, "stat_names", "stat_names must be list")?;
    require_str(c, "clusterer_type", "clusterer_type must be str")?;
    require_one_of(c, "clusterer_type", IMPLEMENTED_CLUSTERERS)?;

    require_int(c, "n_clusters", "n_clusters must be int")?;
    require_str(c, "reducer_type", "reducer type must be str")?;
    // `none` skips the reduction and clusters the aggregated features
    // themselves; it is the only other reducer, because it is the only other
    // one anything implements.
    require_one_of(c, "reducer_type", IMPLEMENTED_REDUCERS)?;

    require_str(c, "metric", "metric must be str")?;
    require_one_of(c, "metric", &["manhattan", "euclidean", "cosine"])?;

    // Numbers, not floats. `min_dist: 0` is the first value of any sweep that
    // walks `min_dist` from zero, and YAML spells it as an integer; refusing it
    // made a well-formed sweep unrunnable for a reason the message did not
    // explain. All three are read with `get_float_or`, which has always
    // accepted either.
    require_number(c, "resolution", "resolution must be a number")?;
    require_int(c, "n_neighbors", "n_neighbors must be int")?;
    require_number(c, "min_dist", "min_dist must be a number")?;
    require_int(c, "dim_clust", "dim_clust must be int")?;
    // Below 1.0 this is a fraction of the cohort, so it cannot be an integer.
    require_number(c, "min_cluster_size", "min_cluster_size must be a number")?;
    require_int(c, "k_cluster", "k_cluster must be int")?;
    require_str(c, "normalize", "normalize must be str")?;
    require_one_of(
        c,
        "normalize",
        &["total", "niche", "obs", "clr", "niche&obs", "all"],
    )?;
    Ok(())
}

/// `stat_funcs` names statistics the aggregation actually computes.
///
/// The aggregation takes the mean and then, optionally, the population standard
/// deviation — [`IMPLEMENTED_STAT_FUNCS`], in that order. A configuration may
/// ask for a prefix of that list and nothing else. `np.std` alone used to be
/// accepted here and silently computed as the mean, because the aggregation
/// counts the statistics rather than reading their names.
fn require_stat_funcs(c: &Value) -> Result<()> {
    require_list(c, "stat_funcs", "stat_funcs must be list")?;
    let asked = split_stat_list(c, "stat_funcs", IMPLEMENTED_STAT_FUNCS);

    let prefix = IMPLEMENTED_STAT_FUNCS
        .iter()
        .take(asked.len())
        .copied()
        .collect::<Vec<_>>();
    if asked.iter().map(String::as_str).eq(prefix.iter().copied()) {
        return Ok(());
    }
    Err(ConfigError::assertion(format!(
        "stat_funcs must be `np.mean` or `np.mean,np.std`, got `{}`",
        asked.join(",")
    )))
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

/// A number, whole or not.
///
/// Used where the value is read with `get_float_or`: demanding a decimal point
/// there is a constraint on the YAML's spelling, not on the parameter.
fn require_number(c: &Value, key: &str, msg: &str) -> Result<()> {
    match c.get(key) {
        Some(Value::Number(_)) => Ok(()),
        _ => Err(ConfigError::assertion(msg)),
    }
}

/// A number, or a string that holds one.
///
/// `order: '1'` is what the interface writes, because its drop-down stores text;
/// `order: 2` is what anything generating a configuration writes.
fn require_number_or_numeric_str(c: &Value, key: &str, msg: &str) -> Result<()> {
    match c.get(key) {
        Some(Value::Number(_)) => Ok(()),
        Some(Value::String(s)) if s.trim().parse::<i64>().is_ok() => Ok(()),
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

    // -----------------------------------------------------------------------
    // The typing traps a generated configuration falls into
    // -----------------------------------------------------------------------

    /// `order` used to be the one integer of the section that had to be
    /// written as a string. Every other integer key is validated with
    /// `require_int`, so a configuration built by a program — which spells an
    /// integer as an integer — was rejected with `order must be str`.
    #[test]
    fn order_accepts_both_an_integer_and_a_string() {
        let quoted = niche_subsection_yaml();
        assert_niche_subsection(&value(&quoted)).unwrap();

        let bare = niche_subsection_yaml().replace("order: '1'", "order: 2");
        assert_niche_subsection(&value(&bare)).expect("an integer order is still an order");
    }

    /// And it is still a number: a word there would silently read back as the
    /// default neighbourhood order.
    #[test]
    fn order_refuses_something_that_is_not_a_number() {
        let yaml = niche_subsection_yaml().replace("order: '1'", "order: first");
        let err = assert_niche_subsection(&value(&yaml)).unwrap_err();
        assert!(err.to_string().contains("order"), "{err}");
    }

    /// `min_dist: 0` is what a sweep from 0 to 0.5 produces for its first
    /// value; it used to be refused because the check demanded a float.
    #[test]
    fn the_float_parameters_accept_a_whole_number() {
        for (key, whole) in [
            ("min_dist: 0.0", "min_dist: 0"),
            ("resolution: 0.05", "resolution: 1"),
        ] {
            let yaml = niche_subsection_yaml().replace(key, whole);
            assert_niche_subsection(&value(&yaml))
                .unwrap_or_else(|e| panic!("`{whole}` was refused: {e}"));
        }
    }

    /// `min_cluster_size` is read as a float — below 1.0 it is a fraction of
    /// the cohort — so the validation may not demand an integer.
    #[test]
    fn min_cluster_size_accepts_a_fraction() {
        let yaml = niche_subsection_yaml().replace("min_cluster_size: 100", "min_cluster_size: 0.001");
        assert_niche_subsection(&value(&yaml)).unwrap();
    }

    /// A parameter that is a number stays a number.
    #[test]
    fn the_numeric_parameters_still_refuse_a_word() {
        let yaml = niche_subsection_yaml().replace("min_dist: 0.0", "min_dist: tight");
        assert!(assert_niche_subsection(&value(&yaml)).is_err());
    }

    // -----------------------------------------------------------------------
    // Only the clusterers that exist
    // -----------------------------------------------------------------------

    /// `ecg` used to pass this check and fail inside the pipeline, after the
    /// aggregation and the reduction had already run — hours, on a real
    /// cohort, to be told the clusterer has no implementation.
    #[test]
    fn a_clusterer_without_an_implementation_is_refused_up_front() {
        for absent in ["ecg", "hdbscan"] {
            let yaml =
                niche_subsection_yaml().replace("clusterer_type: gmm", &format!("clusterer_type: {absent}"));
            let err = assert_niche_subsection(&value(&yaml))
                .expect_err(&format!("`{absent}` was accepted"));
            assert!(err.to_string().contains("clusterer_type must be one of"), "{err}");
        }
    }

    /// And the ones that do exist are all accepted.
    #[test]
    fn every_implemented_clusterer_is_accepted() {
        for present in crate::model::niche_params::IMPLEMENTED_CLUSTERERS {
            let yaml = niche_subsection_yaml()
                .replace("clusterer_type: gmm", &format!("clusterer_type: {present}"));
            assert_niche_subsection(&value(&yaml))
                .unwrap_or_else(|e| panic!("`{present}` was refused: {e}"));
        }
    }

    // -----------------------------------------------------------------------
    // stat_funcs
    // -----------------------------------------------------------------------

    /// Only the mean and the population standard deviation are implemented,
    /// in that order. Anything else would be read as "one statistic" and
    /// silently computed as the mean.
    #[test]
    fn stat_funcs_refuses_a_statistic_nothing_computes() {
        for absent in ["np.median", "np.std", "np.mean,np.median"] {
            let yaml = niche_subsection_yaml()
                .replace("stat_funcs: np.mean,np.std", &format!("stat_funcs: {absent}"));
            assert!(
                assert_niche_subsection(&value(&yaml)).is_err(),
                "`{absent}` was accepted"
            );
        }
    }

    #[test]
    fn stat_funcs_accepts_the_two_shapes_the_interface_offers() {
        for offered in ["np.mean,np.std", "np.mean"] {
            let yaml = niche_subsection_yaml()
                .replace("stat_funcs: np.mean,np.std", &format!("stat_funcs: {offered}"));
            assert_niche_subsection(&value(&yaml))
                .unwrap_or_else(|e| panic!("`{offered}` was refused: {e}"));
        }
    }
}
