//! Configuration layer of MOSNA.
//!
//! Mirrors the Python `package/utils/read_config.py`, `save_config.py` and
//! `assert_params.py` modules. The on-disk format is byte-for-byte the same
//! `CONFIG/configuration.yaml` used by the Python application, so an existing
//! configuration file can be used by either implementation interchangeably.
//!
//! Two views of the configuration coexist on purpose:
//!
//! * [`RawConfig`] keeps the YAML document as an *ordered* tree of values. The
//!   GUI builds its parameter forms by walking that tree, exactly like the
//!   Python GUI walks the `dict` returned by `yaml.safe_load`. Saving from this
//!   view round-trips unknown keys and preserves their original order.
//! * The typed views ([`TysserandConfig`], [`AssortativityConfig`],
//!   [`NicheAnalysisConfig`]) are extracted from the raw tree by the pipelines,
//!   which need strongly-typed access.

pub mod error;
pub mod io;
pub mod model;
pub mod validate;
pub mod value;

pub use error::{ConfigError, Result};
pub use io::{get_config::get_config, save_config::save_config, write_config::write_config};
pub use model::{
    assortativity::AssortativityConfig,
    niche::{NicheAnalysisConfig, ProcessingMethod},
    niche_params::NicheParams,
    raw::RawConfig,
    tysserand::TysserandConfig,
};
pub use validate::assert_params::assert_params;

/// Section names as they appear in `configuration.yaml`.
pub mod section {
    pub const TYSSERAND: &str = "Tysserand";
    pub const ASSORTATIVITY: &str = "Assortativity";
    pub const NICHE_ANALYSIS: &str = "Niche Analysis";
    pub const AGGREGATED_NODES: &str = "Aggregated nodes";
    pub const PER_SAMPLE: &str = "Per sample";
}
