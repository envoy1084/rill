//! Why a `rill-server` command failed.

use thiserror::Error;

/// A command's failure; every variant ends the process with exit 1, a
/// usage error with 2.
#[derive(Debug, Error)]
pub enum Error {
    /// The arguments make no sense together.
    #[error("{0}")]
    Usage(String),

    /// The services failed to start or stopped with an error, or
    /// configuration, telemetry or token minting failed.
    #[error(transparent)]
    Server(#[from] rill_api::Error),

    /// The cache tier failed.
    #[error("cache error: {0}")]
    Cache(#[from] rill_cache::Error),

    /// Local storage failed.
    #[error("storage error: {0}")]
    Store(#[from] rill_store::Error),

    /// A git import failed.
    #[error("import error: {0}")]
    Import(#[from] rill_git_import::Error),

    /// A file could not be read or written.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// A usage error.
    pub fn usage(msg: impl Into<String>) -> Self {
        Error::Usage(msg.into())
    }
}

/// Result alias for commands.
pub type Result<T> = std::result::Result<T, Error>;
