//! Statistical helpers shared across the core.

pub mod clr;
pub mod linkage;
pub mod percentile;

pub use clr::{closure, clr};
pub use linkage::{dendrogram_leaf_order, ward_linkage, Linkage};
pub use percentile::percentile;
