//! SDK error type.

use rill_types::{ChangeId, ChangeVersionHash, RepoPathBuf};
use thiserror::Error;
use tonic::Code;

/// Everything that can go wrong talking to a Rill server or working with a
/// local checkout.
#[derive(Debug, Error)]
pub enum Error {
    /// The server answered with a gRPC status. The code is preserved so
    /// callers can branch on `NotFound`, `AlreadyExists`, `Aborted`, ...
    #[error("{code:?}: {message}")]
    Status {
        /// gRPC status code.
        code: Code,
        /// Server-provided message.
        message: String,
    },

    /// The connection could not be established or was lost.
    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    /// A wire message could not be decoded into a `rill-types` value.
    #[error("conversion error: {0}")]
    Convert(#[from] rill_proto::ConvertError),

    /// A value built on the client side was invalid (bad path, ref name, hash).
    #[error("invalid value: {0}")]
    Invalid(#[from] rill_types::Error),

    /// The client configuration is missing or malformed.
    #[error("configuration error: {0}")]
    Config(String),

    /// A local filesystem operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A client-side wait exceeded its deadline.
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// A server-side operation produced an unresolved conflict where the
    /// caller asked for a clean result.
    #[error("change {change_id} version {hash} has conflicts in {paths:?}")]
    Conflict {
        /// The change whose version conflicts.
        change_id: ChangeId,
        /// The conflicted version.
        hash: ChangeVersionHash,
        /// Paths left in conflict.
        paths: Vec<RepoPathBuf>,
    },

    /// The local checkout metadata is missing or malformed.
    #[error("checkout error: {0}")]
    Checkout(String),

    /// A landing finished without landing (failed or cancelled).
    #[error("land {ticket} ended with {status:?}: {message}")]
    LandFailed {
        /// The land ticket id.
        ticket: String,
        /// Terminal status reported by the server.
        status: crate::types::LandStatus,
        /// Server-provided error text, if any.
        message: String,
    },
}

impl Error {
    /// The gRPC code, if this error came from a status.
    pub fn code(&self) -> Option<Code> {
        match self {
            Error::Status { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// True for a `NotFound` status.
    pub fn is_not_found(&self) -> bool {
        self.code() == Some(Code::NotFound)
    }

    /// Build a configuration error.
    pub fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    /// Build a checkout error.
    pub fn checkout(msg: impl Into<String>) -> Self {
        Error::Checkout(msg.into())
    }
}

impl From<tonic::Status> for Error {
    fn from(status: tonic::Status) -> Self {
        Error::Status {
            code: status.code(),
            message: status.message().to_string(),
        }
    }
}

/// Result alias used throughout the SDK.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_preserves_code() {
        let err: Error = tonic::Status::not_found("no such repo").into();
        assert!(err.is_not_found());
        assert_eq!(err.code(), Some(Code::NotFound));
        assert_eq!(err.to_string(), "NotFound: no such repo");
    }
}
