//! Daemon error type.

use thiserror::Error;

/// Everything that can go wrong in the daemon or talking to it.
#[derive(Debug, Error)]
pub enum Error {
    /// The server (or the daemon relaying it) refused or failed.
    #[error(transparent)]
    Sdk(#[from] rill_sdk::Error),
    /// A local file operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The state database failed.
    #[error("state store: {0}")]
    State(String),
    /// A stored record could not be decoded.
    #[error("encoding: {0}")]
    Encoding(#[from] postcard::Error),
    /// No such mount, or no mount contains the path.
    #[error("{0}")]
    NotFound(String),
    /// A request was malformed.
    #[error("{0}")]
    Invalid(String),
    /// The working copy is in a state that refuses the request (unresolved
    /// conflicts, a busy directory).
    #[error("{0}")]
    Conflict(String),
    /// The platform cannot serve the request (a lazy mount off Linux).
    #[error("{0}")]
    Unsupported(String),
    /// The daemon is not running and could not be started.
    #[error("daemon unavailable: {0}")]
    Unavailable(String),
    /// A filesystem watcher failed.
    #[error("watcher: {0}")]
    Watch(#[from] notify::Error),
    /// A blocking task panicked or was cancelled.
    #[error("task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

impl Error {
    pub(crate) fn state(err: impl std::fmt::Display) -> Self {
        Error::State(err.to_string())
    }
}

impl From<rill_types::Error> for Error {
    fn from(err: rill_types::Error) -> Self {
        Error::Invalid(err.to_string())
    }
}

/// Result alias used throughout the daemon.
pub type Result<T> = std::result::Result<T, Error>;
