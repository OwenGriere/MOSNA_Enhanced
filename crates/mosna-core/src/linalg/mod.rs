//! Dense linear algebra used by the clustering and reduction routines.
//!
//! MOSNA needs three things numpy/scipy provide and which have no lightweight
//! Rust equivalent that avoids a LAPACK build dependency: a symmetric
//! eigensolver (spectral embedding and spectral clustering), a Cholesky
//! factorisation (Gaussian mixture covariances) and a k-means (spectral
//! clustering's final step, and the mixture initialisation).
//!
//! The matrices involved are small — the number of clusters or the embedding
//! dimensionality, never the number of cells — so straightforward dense
//! algorithms are the right choice.

pub mod cholesky;
pub mod eigen;
pub mod kmeans;

pub use cholesky::{cholesky, Cholesky};
pub use eigen::{symmetric_eigen, Eigen};
pub use kmeans::{kmeans, KMeansResult};
