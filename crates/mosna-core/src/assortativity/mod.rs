//! Network assortativity — port of `mosna/assortativity.py`.
//!
//! For each sample the mixing matrix of the phenotype attributes is computed,
//! then compared against a null distribution obtained by shuffling the node
//! attributes while keeping the topology fixed. The z-scores say which pairs of
//! phenotypes are found adjacent more or less often than chance.

pub mod attribute_ac;
pub mod mixing_matrix;
pub mod mixmat_columns;
pub mod randomized_mixmat;
pub mod sample_assort_mixmat;
pub mod zscore;

pub use attribute_ac::attribute_ac;
pub use mixing_matrix::{mixing_matrix, MixMat};
pub use mixmat_columns::{attributes_pairs, mixmat_to_columns, series_to_mixmat};
pub use randomized_mixmat::randomized_mixmat;
pub use sample_assort_mixmat::{sample_assort_mixmat, SampleStats};
pub use zscore::zscore;
