//! Scientific core of MOSNA.
//!
//! Ports the algorithmic content of `mosna-package/mosna` and of the
//! `tysserand` functions the pipelines depend on. Each sub-module maps onto one
//! Python module:
//!
//! | Rust | Python |
//! |---|---|
//! | [`geometry`] | `tysserand.tysserand` (network reconstruction) |
//! | [`nas`] | `mosna.neighbors` (Neighbors Aggregation Statistics) |
//! | [`assortativity`] | `mosna.assortativity` |
//! | [`reduction`] | `mosna.clustering::get_reducer` (UMAP) |
//! | [`clustering`] | `mosna.clustering::get_clusterer` |
//! | [`niches`] | `mosna.niches` |
//! | [`stats`] | percentiles, Ward linkage, CLR |

pub mod assortativity;
pub mod clustering;
pub mod error;
pub mod geometry;
pub mod linalg;
pub mod nas;
pub mod niches;
pub mod reduction;
pub mod spatial;
pub mod stats;

pub use error::{CoreError, Result};

/// An undirected edge between two node indices.
pub type Pair = (u32, u32);

/// A 2-D point.
pub type Point2 = [f64; 2];
