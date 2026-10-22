//! Errors reading the event log.

/// An event log error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),
}

/// A result with an event log error.
pub type Result<T> = std::result::Result<T, Error>;
