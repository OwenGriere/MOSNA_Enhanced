//! Niche clustering — port of `mosna/clustering.py::get_clusterer`.
//!
//! Four algorithms are selectable from the configuration. `leiden` and `gmm`
//! are the ones the shipped configuration uses; `spectral` is available and
//! `ecg` exists only on GPU, where the Python raises for CPU runs.

pub mod gmm;
pub mod leiden;
pub mod merge_clusters;
pub mod relabel_clusters;
pub mod spectral;

pub use gmm::{gaussian_mixture, GmmParams, GmmResult};
pub use leiden::leiden;
pub use merge_clusters::{merge_clusters, merge_clusters_until};
pub use relabel_clusters::relabel_clusters;
pub use spectral::{spectral_clustering, SpectralParams};
