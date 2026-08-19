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

    #[error("failed to write {path}: {source}")]
    Write {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every failure that touches the disk has to name the file. "No such file
    /// or directory (os error 2)" is a message a user cannot act on, and both
    /// of these are reached by pressing a button in the interface.
    #[test]
    fn a_failure_to_write_names_the_file() {
        let error = PipelineError::Write {
            path: std::path::PathBuf::from("/runs/report.html"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        let message = error.to_string();

        assert!(message.contains("/runs/report.html"), "{message}");
        assert!(message.contains("not found"), "{message}");
    }

    #[test]
    fn a_failure_to_create_a_directory_names_it_too() {
        let error = PipelineError::CreateDir {
            path: std::path::PathBuf::from("/runs/Assortativity"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        assert!(error.to_string().contains("/runs/Assortativity"));
    }
}
