//! Locating network files and decoding the identifiers encoded in their names.
//!
//! The naming convention is the contract between every step of the pipeline:
//!
//! ```text
//! nodes_{patient_column}-{patient_id}_{sample_column}-{sample_id}.{extension}
//! edges_{patient_column}-{patient_id}_{sample_column}-{sample_id}.{extension}
//! ```
//!
//! with the `_{sample_column}-{sample_id}` part omitted for single-level
//! datasets. Getting this wrong silently produces empty results, so each
//! function here is a direct port of its Python counterpart and is covered by
//! tests using real file names.

pub mod find_sample;
pub mod find_sample_from_file;
pub mod make_data_index;
pub mod sample_id;
