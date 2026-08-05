//! Spatial network reconstruction — port of the `tysserand` functions used by
//! MOSNA.
//!
//! The reconstruction pipeline of `draw_per_sample` is:
//!
//! ```python
//! pairs = ty.build_delaunay(coords)
//! pairs = ty.link_solitaries(coords, pairs, method=method, min_neighbors=min_neighbors)
//! ```
//!
//! so this module provides [`fn@build_delaunay`] and [`fn@link_solitaries`] plus the
//! helpers they rely on.

pub mod build_delaunay;
pub mod build_knn;
pub mod distance_neighbors;
pub mod find_trim_dist;
pub mod link_solitaries;
pub mod remove_duplicate_pairs;

pub use build_delaunay::{
    build_delaunay, build_delaunay_untrimmed, DelaunayTrim, NodeAdaptive, TrimDist,
};
pub use build_knn::build_knn;
pub use distance_neighbors::distance_neighbors;
pub use find_trim_dist::find_trim_dist;
pub use link_solitaries::{link_solitaries, LinkMethod};
pub use remove_duplicate_pairs::remove_duplicate_pairs;
