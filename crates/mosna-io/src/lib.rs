//! Table I/O and network file discovery.
//!
//! Ports `package/utils/read_extension.py`, `find_sample.py`,
//! `find_sample_from_file.py`, `convert_net_dir.py`,
//! `emit_qt_progress.py` and `mosna/preprocessing.py::make_data_index`.
//!
//! Tables are backed by Arrow arrays rather than a full dataframe engine. The
//! pipelines only ever need columnar reads by name, whole-file round-trips and
//! a handful of appended columns, so keeping the original Arrow arrays means
//! dtypes survive a read/write cycle exactly, the way `pd.read_parquet` /
//! `to_parquet` preserves them on the Python side.

pub mod convert;
pub mod discovery;
pub mod error;
pub mod progress;
pub mod read;
pub mod table;
pub mod write;

pub use convert::convert_net_dir::convert_net_dir;
pub use discovery::{
    find_sample::find_sample, find_sample_from_file::find_sample_from_file,
    make_data_index::make_data_index, sample_id::SampleId,
};
pub use error::{IoError, Result};
pub use progress::emit::{emit_qt_info, emit_qt_progress};
pub use read::{get_opener::read_table, get_opener::Extension};
pub use table::Table;
pub use write::{write_csv::write_csv, write_parquet::write_parquet};
