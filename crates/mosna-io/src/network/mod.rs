//! Networks: the compressed sparse row form, and the two files it is kept in.

pub mod csr;
pub mod format;

pub use csr::{Csr, CsrError, Pair};
pub use format::{
    edges_format, read_network, read_network_files, write_network, EdgesFormat, DEGREE_COLUMN,
    INDICES_COLUMN,
};
