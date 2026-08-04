//! Dimensionality reduction — port of `mosna/clustering.py::get_reducer`.
//!
//! Only UMAP is implemented, because it is the only reducer the configuration
//! can select: `assert_params` asserts `reducer_type in ['umap']`, and the GUI
//! offers no other option.

pub mod umap;

pub use umap::{umap, Metric, UmapParams};
