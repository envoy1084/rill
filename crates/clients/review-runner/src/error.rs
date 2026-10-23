//! Runner errors.

/// A runner error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sdk(#[from] rill_sdk::Error),
    /// The model service failed or answered out of shape.
    #[error("backend: {0}")]
    Backend(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("configuration: {0}")]
    Config(String),
}

/// A result with a runner error.
pub type Result<T> = std::result::Result<T, Error>;
