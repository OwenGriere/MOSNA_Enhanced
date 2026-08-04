//! Error type of the scientific core.

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Io(#[from] mosna_io::IoError),

    /// A table could not be assembled, e.g. because a computed column has the
    /// wrong length for the table it is being added to.
    #[error("arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    /// A network could not be reconstructed from the given coordinates.
    #[error("cannot build a network from {n_points} point(s): {reason}")]
    Geometry { n_points: usize, reason: String },

    /// Shapes that must agree do not.
    #[error("shape mismatch: {0}")]
    Shape(String),

    /// A numerical routine failed to produce a usable result.
    #[error("{algorithm} failed: {reason}")]
    Numeric {
        algorithm: &'static str,
        reason: String,
    },

    /// A requested algorithm exists in the configuration but has no CPU
    /// implementation, mirroring the Python
    /// `raise RuntimeError('ecg clustering requires the cugraph library')`.
    #[error("{0}")]
    Unsupported(String),

    #[error("{0}")]
    Invalid(String),
}

impl CoreError {
    pub fn shape(msg: impl Into<String>) -> Self {
        CoreError::Shape(msg.into())
    }

    pub fn numeric(algorithm: &'static str, reason: impl Into<String>) -> Self {
        CoreError::Numeric {
            algorithm,
            reason: reason.into(),
        }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        CoreError::Invalid(msg.into())
    }
}
