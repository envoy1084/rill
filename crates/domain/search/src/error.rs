use rill_types::TreeHash;
use thiserror::Error;

/// Errors produced by the search index and query layer.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    #[error(transparent)]
    Core(#[from] rill_core::Error),

    #[error(transparent)]
    Types(#[from] rill_types::Error),

    #[error("invalid search pattern: {0}")]
    InvalidPattern(String),

    #[error("invalid path glob: {0}")]
    InvalidGlob(String),

    #[error("root tree {0} is not indexed")]
    RootNotIndexed(TreeHash),

    #[error("index codec error: {0}")]
    Codec(String),

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl From<regex::Error> for Error {
    fn from(e: regex::Error) -> Self {
        Error::InvalidPattern(e.to_string())
    }
}

impl From<globset::Error> for Error {
    fn from(e: globset::Error) -> Self {
        Error::InvalidGlob(e.to_string())
    }
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
