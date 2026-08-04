//! Port of `package/utils/read_config.py::get_config`.

use std::path::Path;

use crate::error::{ConfigError, Result};
use crate::model::raw::RawConfig;

/// Load `configuration.yaml` from disk.
///
/// Equivalent to the Python
/// ```python
/// with open(config_path, 'r') as f:
///     config = yaml.safe_load(f)
/// ```
/// with the addition that key order is preserved, which the GUI relies on to
/// lay its parameter forms out in the same order as the file.
pub fn get_config(config_path: impl AsRef<Path>) -> Result<RawConfig> {
    let path = config_path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    RawConfig::from_yaml_str(&text).map_err(|source| ConfigError::Yaml {
        path: path.to_path_buf(),
        source,
    })
}
