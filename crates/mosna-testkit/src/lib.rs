//! Fixtures, generators and assertions shared by the MOSNA test suites.
//!
//! The port is developed test-first: each algorithm gets its properties written
//! down before the implementation exists, and this crate holds the pieces those
//! tests need in common — synthetic datasets with known structure, proptest
//! strategies for the shapes the algorithms accept, and assertions that state
//! an invariant once instead of at every call site.
//!
//! It is never linked into the shipped binaries: it is a dev-dependency of the
//! crates it serves.

pub mod assertions;
pub mod fixtures;
pub mod strategies;

pub use assertions::{assert_close, assert_slice_close, assert_symmetric, assert_valid_partition};
pub use fixtures::{blobs, cohort, grid, ring, Cohort};
pub use strategies::{finite_f64, point_cloud, small_graph};
