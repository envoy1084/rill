use thiserror::Error;

/// Errors of the cache tier.
#[derive(Debug, Error)]
pub enum Error {
    /// The request carries no grant naming the object, or a bad one.
    #[error("{0}")]
    Denied(String),

    /// The object was redacted at the origin.
    #[error("{0} was redacted")]
    Redacted(String),

    /// The origin does not have the object.
    #[error("{0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    /// The cache cannot serve safely right now.
    #[error("unavailable: {0}")]
    Unavailable(String),

    /// The origin failed or could not be reached.
    #[error("origin: {0}")]
    Origin(String),

    /// The origin served bytes that do not hash to the object.
    #[error("origin served a corrupt object: {0}")]
    Corrupt(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("cache configuration: {0}")]
    Config(String),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
