//! Neighbors Aggregation Statistics — port of `mosna/neighbors.py`.
//!
//! For every node, the attributes of the nodes within `order` hops (the node
//! itself included) are aggregated with a set of statistics — by default the
//! mean and the population standard deviation. The resulting vector is what the
//! niche clustering runs on.

pub mod adjacency;
pub mod bfs;
pub mod make_features_nas;
pub mod onehot;
pub mod spatial_omic_features;

pub use adjacency::Adjacency;
pub use make_features_nas::{make_features_nas, NasFeatures};
pub use onehot::one_hot;
pub use spatial_omic_features::{
    compute_spatial_omic_features_all_networks, compute_spatial_omic_features_single_network,
    SofOptions,
};
