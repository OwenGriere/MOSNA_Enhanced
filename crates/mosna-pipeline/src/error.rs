//! Error type of the pipeline layer.

pub type Result<T> = std::result::Result<T, PipelineError>;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    Config(#[from] mosna_config::ConfigError),

    #[error(transparent)]
    Io(#[from] mosna_io::IoError),

    #[error(transparent)]
    Core(#[from] mosna_core::CoreError),

    /// A computed table could not be assembled from its columns.
    #[error("arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    #[error("failed to create {path}: {source}")]
    CreateDir {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to remove {path}: {source}")]
    Remove {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// No sample matched the configured naming pattern.
    #[error("no network files found in {path} matching `nodes_{pattern}`")]
    NoSamples {
        path: std::path::PathBuf,
        pattern: String,
    },

    #[error("{0}")]
    Invalid(String),
}

impl PipelineError {
    pub fn invalid(message: impl Into<String>) -> Self {
        PipelineError::Invalid(message.into())
    }
}

/// Create a directory and every missing parent, naming it on failure.
pub fn create_dir_all(path: impl AsRef<std::path::Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path).map_err(|source| PipelineError::CreateDir {
        path: path.to_path_buf(),
        source,
    })
}
