//! One specification builder per figure.
//!
//! Each of these turns what an analysis computed into the document the
//! renderer draws from. They are where the *decisions* live — which colour
//! map, what the title says, which pairs are shown, what a cell is normalised
//! against — because those are the things this project's tests already pin,
//! and because a charting library is a poor place to keep them.

pub mod abundance;
pub mod composition;
pub mod embedding;
pub mod heatmap;
pub mod histogram;
pub mod mean_std;
pub mod mixing_matrix;
pub mod network;
