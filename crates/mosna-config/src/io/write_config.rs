//! Port of the YAML dump performed by `GUI_MOSNA.py::MosnaGUI._save_config`.

use std::path::Path;

use crate::error::{ConfigError, Result};
use crate::model::raw::RawConfig;

/// Write the configuration back to `config_path`.
///
/// The Python GUI dumps with `sort_keys=False` and a custom representer that
/// forces every list to flow style (`[mean, std]` rather than a block
/// sequence). [`RawConfig::to_yaml_string`] reproduces both behaviours so the
/// file stays diff-clean when edited by either implementation.
pub fn write_config(config: &RawConfig, config_path: impl AsRef<Path>) -> Result<()> {
    let path = config_path.as_ref();
    let text = config
        .to_yaml_string()
        .map_err(|source| ConfigError::Yaml {
            path: path.to_path_buf(),
            source,
        })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, text).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}
