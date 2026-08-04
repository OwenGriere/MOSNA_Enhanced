//! Typed view of a niche sub-section (`Aggregated nodes` / `Per sample`).

use serde_yaml::Value;

use crate::value::{get_float_or, get_int_or, get_str_or};

/// Dimensionality reduction algorithm applied before clustering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReducerType {
    Umap,
    /// Cluster the raw features without reduction.
    None,
}

impl ReducerType {
    pub fn parse(s: &str) -> Self {
        match s {
            "none" => ReducerType::None,
            _ => ReducerType::Umap,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ReducerType::Umap => "umap",
            ReducerType::None => "none",
        }
    }
}

/// Clustering algorithm used to call niches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClustererType {
    Leiden,
    Ecg,
    Spectral,
    Gmm,
    Hdbscan,
}

impl ClustererType {
    pub fn parse(s: &str) -> Self {
        match s {
            "ecg" => ClustererType::Ecg,
            "spectral" => ClustererType::Spectral,
            "gmm" => ClustererType::Gmm,
            "hdbscan" => ClustererType::Hdbscan,
            _ => ClustererType::Leiden,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ClustererType::Leiden => "leiden",
            ClustererType::Ecg => "ecg",
            ClustererType::Spectral => "spectral",
            ClustererType::Gmm => "gmm",
            ClustererType::Hdbscan => "hdbscan",
        }
    }
}

/// Distance metric used by the reducer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Euclidean,
    Manhattan,
    Cosine,
}

impl Metric {
    pub fn parse(s: &str) -> Self {
        match s {
            "manhattan" => Metric::Manhattan,
            "cosine" => Metric::Cosine,
            _ => Metric::Euclidean,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Euclidean => "euclidean",
            Metric::Manhattan => "manhattan",
            Metric::Cosine => "cosine",
        }
    }
}

/// Normalisation applied to the niche composition matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Normalize {
    Total,
    Niche,
    Obs,
    Clr,
    NicheAndObs,
    /// Produce one figure set per normalisation.
    All,
}

impl Normalize {
    pub fn parse(s: &str) -> Self {
        match s {
            "niche" => Normalize::Niche,
            "obs" => Normalize::Obs,
            "clr" => Normalize::Clr,
            "niche&obs" => Normalize::NicheAndObs,
            "all" => Normalize::All,
            _ => Normalize::Total,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Normalize::Total => "total",
            Normalize::Niche => "niche",
            Normalize::Obs => "obs",
            Normalize::Clr => "clr",
            Normalize::NicheAndObs => "niche&obs",
            Normalize::All => "all",
        }
    }

    /// The normalisations to actually compute for this setting.
    pub fn expand(self) -> Vec<Normalize> {
        match self {
            Normalize::All => vec![
                Normalize::Total,
                Normalize::Niche,
                Normalize::Obs,
                Normalize::Clr,
                Normalize::NicheAndObs,
            ],
            other => vec![other],
        }
    }
}

/// Reduction, clustering and normalisation settings of one niche method.
#[derive(Debug, Clone)]
pub struct NicheParams {
    pub reducer_type: ReducerType,
    pub clusterer_type: ClustererType,
    pub metric: Metric,
    pub normalize: Normalize,
    pub n_neighbors: usize,
    pub min_dist: f64,
    pub dim_clust: usize,
    pub k_cluster: usize,
    pub n_clusters: usize,
    pub resolution: f64,
    /// Below 1.0 this is a fraction of the dataset size, as in HDBSCAN's
    /// `min_cluster_size` handling in `clustering.py`.
    pub min_cluster_size: f64,
    /// Neighbourhood order for the NAS aggregation.
    pub order: usize,
    pub stat_funcs: Vec<String>,
    pub stat_names: Vec<String>,
}

impl NicheParams {
    /// Read a sub-section, applying the same defaults as `niche_analysis.py`.
    pub fn from_value(section: &Value) -> Self {
        Self {
            reducer_type: ReducerType::parse(&get_str_or(section, "reducer_type", "umap")),
            clusterer_type: ClustererType::parse(&get_str_or(section, "clusterer_type", "leiden")),
            metric: Metric::parse(&get_str_or(section, "metric", "euclidean")),
            normalize: Normalize::parse(&get_str_or(section, "normalize", "total")),
            n_neighbors: get_int_or(section, "n_neighbors", 15).max(2) as usize,
            min_dist: get_float_or(section, "min_dist", 0.0),
            dim_clust: get_int_or(section, "dim_clust", 2).max(1) as usize,
            k_cluster: get_int_or(section, "k_cluster", 8).max(1) as usize,
            n_clusters: get_int_or(section, "n_clusters", 15).max(1) as usize,
            resolution: get_float_or(section, "resolution", 0.005),
            min_cluster_size: get_float_or(section, "min_cluster_size", 0.001),
            order: get_int_or(section, "order", 1).max(1) as usize,
            stat_funcs: split_stat_list(section, "stat_funcs", &["np.mean", "np.std"]),
            stat_names: split_stat_list(section, "stat_names", &["mean", "std"]),
        }
    }

    /// `k_cluster` capped by `n_neighbors`, reproducing the
    /// `avoid_neigh_overflow` guard of `get_clusterer`.
    pub fn effective_k_cluster(&self) -> usize {
        self.k_cluster.min(self.n_neighbors)
    }

    /// Directory name encoding the reduction settings, matching
    /// `clustering.py::make_reducer_name` so that cached embeddings computed by
    /// either implementation are found by the other.
    pub fn reducer_name(&self) -> String {
        match self.reducer_type {
            ReducerType::Umap => format!(
                "reducer-umap_dim-{}_nneigh-{}_metric-{}_min_dist-{}",
                self.dim_clust,
                self.n_neighbors,
                self.metric.as_str(),
                format_python_float(self.min_dist),
            ),
            ReducerType::None => "reducer-none".to_string(),
        }
    }

    /// File stem encoding the clustering settings, matching the
    /// `clusterer_name` local of `get_clusterer`.
    pub fn clusterer_name(&self) -> String {
        match self.clusterer_type {
            ClustererType::Leiden => {
                format!("leiden_resolution-{}", format_python_float(self.resolution))
            }
            ClustererType::Ecg => "ecg_min_weight-0.05_ensemble_size-20".to_string(),
            ClustererType::Spectral => format!("spectral_n_clusters-{}", self.n_clusters),
            ClustererType::Gmm => format!("gmm_n_clusters-{}", self.n_clusters),
            ClustererType::Hdbscan => format!(
                "hdbscan_min_cluster_size-{}_noise_to_cluster-False",
                format_python_float(self.min_cluster_size)
            ),
        }
    }

    /// Sub-directory holding the clustering artefacts, relative to the reducer
    /// directory.
    pub fn cluster_dir_name(&self) -> String {
        match self.clusterer_type {
            ClustererType::Leiden | ClustererType::Ecg | ClustererType::Spectral => format!(
                "clusterer-{}_n_neighbors-{}",
                self.clusterer_type.as_str(),
                self.effective_k_cluster()
            ),
            other => format!("clusterer-{}", other.as_str()),
        }
    }
}

/// Render a float the way Python's `str()` does, so that path names match.
///
/// Python prints `0.0`, `0.05` and `100.0`; Rust's `{}` prints `0`, `0.05` and
/// `100`. Only the trailing `.0` differs, and only that needs fixing here
/// because the values involved are small decimals.
fn format_python_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e16 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// Read a stat list, accepting both a YAML list and a comma-separated scalar.
fn split_stat_list(section: &Value, key: &str, default: &[&str]) -> Vec<String> {
    let fallback = || default.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match section.get(key) {
        Some(Value::Sequence(seq)) => {
            let items: Vec<String> = seq
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if items.is_empty() {
                fallback()
            } else {
                items
            }
        }
        Some(Value::String(s)) => {
            let items: Vec<String> = s
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            if items.is_empty() {
                fallback()
            } else {
                items
            }
        }
        _ => fallback(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(yaml: &str) -> NicheParams {
        NicheParams::from_value(&serde_yaml::from_str(yaml).unwrap())
    }

    #[test]
    fn reducer_name_matches_python() {
        let p = params(
            "reducer_type: umap\ndim_clust: 2\nn_neighbors: 20\nmetric: manhattan\nmin_dist: 0.0\n",
        );
        assert_eq!(
            p.reducer_name(),
            "reducer-umap_dim-2_nneigh-20_metric-manhattan_min_dist-0.0"
        );
    }

    #[test]
    fn clusterer_name_matches_python() {
        let gmm = params("clusterer_type: gmm\nn_clusters: 6\n");
        assert_eq!(gmm.clusterer_name(), "gmm_n_clusters-6");
        assert_eq!(gmm.cluster_dir_name(), "clusterer-gmm");

        let leiden =
            params("clusterer_type: leiden\nresolution: 0.05\nk_cluster: 20\nn_neighbors: 20\n");
        assert_eq!(leiden.clusterer_name(), "leiden_resolution-0.05");
        assert_eq!(leiden.cluster_dir_name(), "clusterer-leiden_n_neighbors-20");
    }

    #[test]
    fn k_cluster_is_capped_by_n_neighbors() {
        let p = params("k_cluster: 50\nn_neighbors: 20\n");
        assert_eq!(p.effective_k_cluster(), 20);
    }

    #[test]
    fn stat_lists_accept_both_shapes() {
        let listed = params("stat_names: [mean, std]\n");
        assert_eq!(listed.stat_names, vec!["mean", "std"]);
        let joined = params("stat_funcs: np.mean,np.std\n");
        assert_eq!(joined.stat_funcs, vec!["np.mean", "np.std"]);
    }

    #[test]
    fn normalize_all_expands_to_every_variant() {
        assert_eq!(Normalize::All.expand().len(), 5);
        assert_eq!(Normalize::Clr.expand(), vec![Normalize::Clr]);
    }
}
