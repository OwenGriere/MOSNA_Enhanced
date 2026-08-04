//! Error type of the configuration layer.

use std::path::PathBuf;

/// Convenience alias used across this crate.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Every failure mode of reading, validating or writing a MOSNA configuration.
///
/// The `Assertion` variant carries the same messages as the Python
/// `assert_params` statements, so the GUI surfaces identical diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read configuration file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write configuration file {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid YAML in {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("missing section `{0}` in configuration")]
    MissingSection(String),

    #[error("missing key `{key}` in section `{section}`")]
    MissingKey { section: String, key: String },

    #[error("key `{key}` in section `{section}` has type {found}, expected {expected}")]
    WrongType {
        section: String,
        key: String,
        expected: &'static str,
        found: &'static str,
    },

    /// Direct translation of a Python `assert ..., "message"` failure.
    #[error("{0}")]
    Assertion(String),
}

impl ConfigError {
    /// Build an [`ConfigError::Assertion`] the way the Python code does.
    pub fn assertion(msg: impl Into<String>) -> Self {
        ConfigError::Assertion(msg.into())
    }
}
