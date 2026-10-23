//! Job queue errors.

/// A job queue error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),
    /// The claim expired and the job was taken back.
    #[error("claim on job {0} was lost")]
    Lost(rill_types::JobId),
    /// A claim handed back by its holder is not one this queue issued.
    #[error("not a claim of queue {0}")]
    Foreign(String),
}

/// A result with a job queue error.
pub type Result<T> = std::result::Result<T, Error>;
