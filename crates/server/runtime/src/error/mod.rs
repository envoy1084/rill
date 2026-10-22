//! Server error type and its mapping onto the wire.
//!
//! Every lower crate's error converts into [`Error`] via `From`, and
//! [`Error`] converts into a `ConnectError` (and a tonic `Status` for
//! REAPI) so handlers can use `?` all the way down. The status code and the
//! reason come from the *meaning* of the underlying error (not found,
//! conflict, bad input) rather than from which crate produced it.

mod codes;
mod reasons;
mod wire;

use rill_proto::ConvertError;
use thiserror::Error;

/// Everything that can go wrong inside the server.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    #[error(transparent)]
    Core(#[from] rill_core::Error),

    #[error(transparent)]
    Policy(#[from] rill_policy::Error),

    #[error(transparent)]
    Review(#[from] rill_review::error::Error),

    #[error(transparent)]
    Lander(#[from] rill_lander::Error),

    #[error(transparent)]
    Events(#[from] rill_events::Error),

    #[error(transparent)]
    Jobs(#[from] rill_jobs::Error),

    #[error(transparent)]
    Exec(#[from] rill_exec::Error),

    #[error(transparent)]
    Search(#[from] rill_search::Error),

    #[error(transparent)]
    Identity(#[from] rill_identity::Error),

    #[error(transparent)]
    Types(#[from] rill_types::Error),

    #[error(transparent)]
    Convert(#[from] ConvertError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("{0}")]
    NotFound(String),

    /// A budget or rate limit was reached.
    #[error("{0}")]
    Exhausted(String),

    #[error("{principal} is sending requests too fast; retry in {retry_after_secs}s")]
    RateLimited {
        principal: String,
        retry_after_secs: u64,
    },

    #[error("{0}")]
    AlreadyExists(String),

    #[error("{0}")]
    InvalidArgument(String),

    #[error("{0}")]
    FailedPrecondition(String),

    #[error("{0}")]
    PermissionDenied(String),

    #[error("{0}")]
    Unauthenticated(String),

    #[error("{0}")]
    Unimplemented(String),

    /// Retry later, possibly against another server.
    #[error("{0}")]
    Unavailable(String),

    #[error("{0}")]
    Internal(String),

    /// A resource name that does not parse.
    #[error("{0}")]
    InvalidName(String),

    /// A revision that does not parse or names nothing.
    #[error("{0}")]
    InvalidRevision(String),

    /// A list filter that does not parse or names an unknown field.
    #[error("invalid filter: {0}")]
    InvalidFilter(String),

    /// A page token that is malformed or was issued for other parameters.
    #[error("invalid page token: {0}")]
    InvalidPageToken(String),

    /// A `request_id` already used for a different request.
    #[error("request_id was already used for a different request")]
    RequestIdReused,

    /// Objects the request refers to were never uploaded.
    #[error("{0}")]
    MissingObjects(String),

    /// The `etag` given is not the resource's current one.
    #[error("{0}")]
    EtagMismatch(String),

    /// The repository's policy does not allow this yet; `missing` says why.
    #[error("{message}: {}", missing.join("; "))]
    PolicyDenied {
        message: String,
        missing: Vec<String>,
    },

    #[error("transport error: {0}")]
    Transport(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// A `NOT_FOUND` error with a message.
    pub fn not_found(msg: impl Into<String>) -> Self {
        Error::NotFound(msg.into())
    }

    /// An `INVALID_ARGUMENT` error with a message.
    /// Whether the content asked for was redacted.
    pub fn is_redacted(&self) -> bool {
        match self {
            Error::Store(rill_store::Error::Redacted { .. }) => true,
            Error::Core(e) => e.is_redacted(),
            _ => false,
        }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Error::InvalidArgument(msg.into())
    }

    /// A `FAILED_PRECONDITION` error with a message.
    pub fn precondition(msg: impl Into<String>) -> Self {
        Error::FailedPrecondition(msg.into())
    }

    /// A `PERMISSION_DENIED` error with a message.
    pub fn denied(msg: impl Into<String>) -> Self {
        Error::PermissionDenied(msg.into())
    }

    /// An `INTERNAL` error with a message.
    pub fn internal(msg: impl Into<String>) -> Self {
        Error::Internal(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use tonic::{Code, Status};

    use super::*;

    #[test]
    fn maps_meaning_to_code() {
        let e: Error = rill_store::Error::NotFound("x".into()).into();
        assert_eq!(e.code(), Code::NotFound);
        let e: Error = rill_core::Error::Store(rill_store::Error::Conflict("x".into())).into();
        assert_eq!(e.code(), Code::Aborted);
        let e: Error =
            rill_review::error::Error::Store(rill_store::Error::NotFound("x".into())).into();
        assert_eq!(e.code(), Code::NotFound);
        assert_eq!(Error::denied("no").code(), Code::PermissionDenied);
        let status: Status = Error::not_found("gone").into();
        assert_eq!(status.code(), Code::NotFound);
        assert_eq!(status.message(), "gone");
    }
}
