//! SDK error type.

use buffa::{Enumeration, Message};
use connectrpc::ErrorCode as Code;
use rill_proto::google::rpc;
pub use rill_proto::rill::v1::ErrorReason;
use rill_types::{ChangeId, ChangeVersionHash, RepoPathBuf};
use thiserror::Error;

/// Everything that can go wrong talking to a Rill server or working with a
/// local checkout.
#[derive(Debug, Error)]
pub enum Error {
    /// The server answered with an error, or the connection failed
    /// (`Unavailable`). The code is preserved so callers can branch on
    /// `NotFound`, `AlreadyExists`, `Aborted`, ...
    #[error("{code:?}: {message}")]
    Status {
        /// Error code (the gRPC codes).
        code: Code,
        /// Server-provided message.
        message: String,
        /// Why, from the error's `ErrorInfo`; `None` from a transport
        /// failure or an older server.
        reason: Option<ErrorReason>,
        /// For a policy denial, what is still missing.
        missing: Vec<String>,
        /// How long to wait before retrying, when the server says.
        retry_after: Option<std::time::Duration>,
    },

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
    /// The error code, if the server or the connection reported one.
    pub fn code(&self) -> Option<Code> {
        match self {
            Error::Status { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// The reason the server gave, if any.
    pub fn reason(&self) -> Option<ErrorReason> {
        match self {
            Error::Status { reason, .. } => *reason,
            _ => None,
        }
    }

    /// Build a status error from a code and message, with no details.
    pub fn status(code: Code, message: impl Into<String>) -> Self {
        Error::Status {
            code,
            message: message.into(),
            reason: None,
            missing: Vec::new(),
            retry_after: None,
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

impl From<connectrpc::ConnectError> for Error {
    fn from(err: connectrpc::ConnectError) -> Self {
        let mut reason = None;
        let mut missing = Vec::new();
        let mut retry_after = None;
        for detail in &err.details {
            let Some(bytes) = detail.value.as_deref().and_then(decode_detail) else {
                continue;
            };
            match detail.type_url.rsplit('/').next().unwrap_or_default() {
                "google.rpc.ErrorInfo" => {
                    if let Ok(info) = rpc::ErrorInfo::decode_from_slice(&bytes) {
                        if info.domain == rill_proto::ERROR_DOMAIN {
                            reason = ErrorReason::from_proto_name(&format!(
                                "ERROR_REASON_{}",
                                info.reason
                            ));
                        }
                    }
                }
                "google.rpc.PreconditionFailure" => {
                    if let Ok(failure) = rpc::PreconditionFailure::decode_from_slice(&bytes) {
                        missing = failure
                            .violations
                            .into_iter()
                            .map(|v| v.description)
                            .collect();
                    }
                }
                "google.rpc.RetryInfo" => {
                    if let Ok(retry) = rpc::RetryInfo::decode_from_slice(&bytes) {
                        if let Some(delay) = retry.retry_delay.as_option() {
                            retry_after =
                                Some(std::time::Duration::from_secs(delay.seconds.max(0) as u64));
                        }
                    }
                }
                _ => {}
            }
        }
        Error::Status {
            code: err.code,
            message: err.message.unwrap_or_default(),
            reason,
            missing,
            retry_after,
        }
    }
}

/// An error detail's value: base64, padded or not.
fn decode_detail(value: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(value.trim_end_matches('='))
        .ok()
}

/// Result alias used throughout the SDK.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_preserves_code() {
        let err: Error = connectrpc::ConnectError::not_found("no such repo").into();
        assert_eq!(err.reason(), None, "no ErrorInfo, no reason");
        assert!(err.is_not_found());
        assert_eq!(err.code(), Some(Code::NotFound));
        assert_eq!(err.to_string(), "NotFound: no such repo");
    }
}
