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

    /// Name of the cached feature table these settings would produce.
    ///
    /// # What has to be in it, and why
    ///
    /// The cache exists so a second run does not recompute the aggregation; it
    /// is only safe if two configurations that would aggregate *differently*
    /// cannot land on the same name. Four things change the table: the
    /// statistics taken over each neighbourhood, the columns aggregated, the
    /// neighbourhood order, and the phenotype vocabulary — the last because a
    /// one-hot encoding has one column per phenotype, so a cohort that gained a
    /// phenotype produces a wider table describing different things.
    ///
    /// The vocabulary enters as a count rather than as a list: the names would
    /// not fit, and a run whose vocabulary changed without changing size is one
    /// where the same cohort was relabelled, which is a different `net_dir` and
    /// so a different working directory.
    pub fn features_stem(&self, columns: &[String], n_phenotypes: usize) -> String {
        let stats = self
            .stat_names
            .iter()
            .map(|s| slug(s))
            .collect::<Vec<_>>()
            .join("_");
        let cols = columns
            .iter()
            .map(|c| slug(c))
            .collect::<Vec<_>>()
            .join("_");
        shorten(format!(
            "var_{stats}_{cols}_order{}_{n_phenotypes}pheno",
            self.order
        ))
    }

    /// The reduction's settings, as the register records them.
    ///
    /// Spelled out rather than encoded into a slug: this is what tells a reader
    /// six months later what `red_1` was, and what two runs are compared on
    /// when one asks whether it has been run before. A name is for finding a
    /// file; these are for knowing what is in it.
    pub fn reduction_parameters(&self) -> serde_json::Value {
        match self.reducer_type {
            ReducerType::Umap => serde_json::json!({
                "reducer_type": "umap",
                "dim_clust": self.dim_clust,
                "n_neighbors": self.n_neighbors,
                "metric": self.metric.as_str(),
                "min_dist": self.min_dist,
            }),
            ReducerType::None => serde_json::json!({ "reducer_type": "none" }),
        }
    }

    /// The clusterer's settings, on the same terms as
    /// [`fn@Self::reduction_parameters`].
    ///
    /// Only the settings that clusterer actually reads: `resolution` means
    /// nothing to GMM, and recording it would make two identical GMM runs look
    /// different because a parameter neither of them used had moved.
    pub fn clustering_parameters(&self) -> serde_json::Value {
        let kind = self.clusterer_type.as_str();
        match self.clusterer_type {
            ClustererType::Leiden => serde_json::json!({
                "clusterer_type": kind,
                "k_cluster": self.effective_k_cluster(),
                "resolution": self.resolution,
            }),
            ClustererType::Ecg => serde_json::json!({
                "clusterer_type": kind,
                "k_cluster": self.effective_k_cluster(),
            }),
            ClustererType::Spectral | ClustererType::Gmm => serde_json::json!({
                "clusterer_type": kind,
                "n_clusters": self.n_clusters,
            }),
            ClustererType::Hdbscan => serde_json::json!({
                "clusterer_type": kind,
                "min_cluster_size": self.min_cluster_size,
            }),
        }
    }

    /// The aggregation's settings, on the same terms.
    ///
    /// The phenotype vocabulary enters as a count: a one-hot encoding has one
    /// column per phenotype, so a cohort that gained one aggregates into a
    /// different table under settings that are otherwise identical.
    pub fn features_parameters(
        &self,
        columns: &[String],
        n_phenotypes: usize,
    ) -> serde_json::Value {
        serde_json::json!({
            "stat_names": self.stat_names,
            "column_to_aggregate": columns,
            "order": self.order,
            "n_phenotypes": n_phenotypes,
        })
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
/// Longest stem a cache file may carry before it is shortened.
///
/// The stems below chain a stage's parameters onto those of every stage it was
/// computed from, so they grow with the pipeline rather than being bounded by
/// it: a cohort aggregating six columns under four statistics reaches this on
/// its own. Most filesystems stop at 255 bytes, and a name that long has
/// stopped being readable well before it stops being legal.
const MAX_STEM: usize = 120;

/// Shorten `stem` to [`MAX_STEM`], keeping it unique.
///
/// The tail is replaced by a digest of the whole, so two stems that agree on
/// their first hundred-odd characters and differ later still land on different
/// files. The digest is FNV-1a: not a cryptographic hash, and it does not need
/// to be — it separates names a user wrote, not names an adversary chose.
fn shorten(stem: String) -> String {
    if stem.len() <= MAX_STEM {
        return stem;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in stem.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    // Cut on a character boundary: a stem can hold any column name the data
    // does, and slicing a multi-byte one in half would panic.
    let mut cut = MAX_STEM - 17;
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}-{hash:016x}", &stem[..cut])
}

/// Make `value` safe to put in a file name.
///
/// Column names come from the user's data, so they hold whatever the instrument
/// wrote: spaces, slashes, plus signs. Only what is unambiguous in a path is
/// kept, and everything else becomes an underscore — this is a name, not an
/// encoding, and the stems stay distinct because the parameters around them do.
fn slug(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

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
