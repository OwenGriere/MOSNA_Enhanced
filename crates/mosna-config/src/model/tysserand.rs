//! Typed view of the `Tysserand` section.

use serde_yaml::Value;

use crate::error::Result;
use crate::model::raw::RawConfig;
use crate::section;
use crate::value::{get_float_or, get_int, get_int_or, get_opt_str, get_str};

/// Method used to draw the edges of a spatial network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgesMethod {
    Delaunay,
    Knn,
}

impl EdgesMethod {
    pub fn parse(s: &str) -> Self {
        match s {
            "knn" => EdgesMethod::Knn,
            // `delaunay` is the default in the shipped configuration, and the
            // Python `link_solitaries` falls back to it for any other value.
            _ => EdgesMethod::Delaunay,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EdgesMethod::Delaunay => "delaunay",
            EdgesMethod::Knn => "knn",
        }
    }
}

/// Key of the per-node trimming ratio (`trim_dist_ratio` in tysserand).
pub const TRIM_DISTANCE_RATIO: &str = "Trim distance ratio";
/// Key of the reference edge rank (`n_edges` in tysserand).
pub const TRIM_NUMBER_OF_EDGES: &str = "Trim number of edges";
/// Key of the length below which no edge is trimmed (`min_dist` in tysserand).
pub const TRIM_MIN_DISTANCE: &str = "Trim min distance";

/// Key of the global length cut-off (`trim_dist` in tysserand).
pub const TRIM_DISTANCE: &str = "Trim distance";

/// The four Delaunay trimming keys, in the order the interface shows them,
/// which is also the order tysserand applies them: the per-node rule, then the
/// global cut-off.
pub const TRIM_KEYS: [&str; 4] = [
    TRIM_DISTANCE_RATIO,
    TRIM_NUMBER_OF_EDGES,
    TRIM_MIN_DISTANCE,
    TRIM_DISTANCE,
];

/// tysserand 0.6.0's `build_delaunay` defaults, which an absent key falls back
/// to: `trim_dist_ratio=2`, `n_edges=3`, `min_dist=0`.
pub const DEFAULT_TRIM_DISTANCE_RATIO: f64 = 2.0;
pub const DEFAULT_TRIM_NUMBER_OF_EDGES: usize = 3;
pub const DEFAULT_TRIM_MIN_DISTANCE: f64 = 0.0;

/// Parameters driving step 1, the spatial network reconstruction.
#[derive(Debug, Clone)]
pub struct TysserandConfig {
    pub nodes_directory: String,
    pub patient_column: String,
    /// `None` when the dataset has a single identifier level.
    pub sample_column: Option<String>,
    pub extension: String,
    pub x_column: String,
    pub y_column: String,
    pub phenotype_column: String,
    pub edges_method: EdgesMethod,
    pub min_neighbors: usize,
    /// Per-node trimming of the Delaunay edges: an edge longer than
    /// `trim_distance_ratio` times its node's `trim_number_of_edges`-th
    /// shortest edge, and longer than `trim_min_distance`, is dropped.
    ///
    /// The three keys are optional, so a configuration written before they
    /// existed builds the network it always did.
    pub trim_distance_ratio: f64,
    pub trim_number_of_edges: usize,
    pub trim_min_distance: f64,
    /// Global cut-off applied after the per-node rule: every edge at or above
    /// this length is dropped, wherever it is (`trim_dist=<float>`).
    ///
    /// `None` — key absent, null or `false` — keeps tysserand's default
    /// `trim_dist=False`, no global cut-off.
    pub trim_distance: Option<f64>,
    pub cpu: usize,
}

impl TysserandConfig {
    /// Extract and type-check the section.
    pub fn from_raw(config: &RawConfig) -> Result<Self> {
        let s = config.section(section::TYSSERAND)?;
        let name = section::TYSSERAND;
        Ok(Self {
            nodes_directory: get_str(s, name, "Nodes directory")?,
            patient_column: get_str(s, name, "Patient column name")?,
            sample_column: get_opt_str(s, "Sample column name"),
            extension: get_str(s, name, "Extension")?,
            x_column: get_str(s, name, "X coordinates column")?,
            y_column: get_str(s, name, "Y coordinates column")?,
            phenotype_column: get_str(s, name, "Phenotype column")?,
            edges_method: EdgesMethod::parse(&get_str(s, name, "Edges method")?),
            min_neighbors: get_int(s, name, "Min neighbors")?.max(0) as usize,
            trim_distance_ratio: get_float_or(s, TRIM_DISTANCE_RATIO, DEFAULT_TRIM_DISTANCE_RATIO),
            trim_number_of_edges: get_int_or(
                s,
                TRIM_NUMBER_OF_EDGES,
                DEFAULT_TRIM_NUMBER_OF_EDGES as i64,
            )
            .max(1) as usize,
            trim_min_distance: get_float_or(s, TRIM_MIN_DISTANCE, DEFAULT_TRIM_MIN_DISTANCE),
            trim_distance: match s.get(TRIM_DISTANCE) {
                Some(Value::Number(n)) => n.as_f64(),
                Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
                _ => None,
            },
            cpu: get_int(s, name, "CPU")?.max(1) as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "\
Tysserand:
  Nodes directory: /data
  Patient column name: patient
  Sample column name: null
  Extension: parquet
  X coordinates column: x
  Y coordinates column: y
  Phenotype column: Cluster
  Edges method: delaunay
  Min neighbors: 3
  CPU: 4
";

    /// A configuration without the trimming keys builds the tysserand 0.6.0
    /// network, as it did before the keys existed.
    #[test]
    fn absent_trimming_keys_fall_back_to_the_tysserand_defaults() {
        let config = TysserandConfig::from_raw(&RawConfig::from_yaml_str(BASE).unwrap()).unwrap();
        assert_eq!(config.trim_distance_ratio, 2.0);
        assert_eq!(config.trim_number_of_edges, 3);
        assert_eq!(config.trim_min_distance, 0.0);
        assert_eq!(config.trim_distance, None);
    }

    #[test]
    fn the_global_cut_off_is_read_and_off_when_null_or_false() {
        let read = |line: &str| {
            let yaml = format!("{BASE}  {line}\n");
            TysserandConfig::from_raw(&RawConfig::from_yaml_str(&yaml).unwrap())
                .unwrap()
                .trim_distance
        };
        assert_eq!(read("Trim distance: 200"), Some(200.0));
        assert_eq!(read("Trim distance: 150.5"), Some(150.5));
        assert_eq!(read("Trim distance: null"), None);
        assert_eq!(read("Trim distance: false"), None);
    }

    #[test]
    fn the_trimming_keys_are_read() {
        let yaml = format!(
            "{BASE}  Trim distance ratio: 3\n  Trim number of edges: 4\n  Trim min distance: 12.5\n"
        );
        let config = TysserandConfig::from_raw(&RawConfig::from_yaml_str(&yaml).unwrap()).unwrap();
        assert_eq!(config.trim_distance_ratio, 3.0);
        assert_eq!(config.trim_number_of_edges, 4);
        assert_eq!(config.trim_min_distance, 12.5);
    }
}
