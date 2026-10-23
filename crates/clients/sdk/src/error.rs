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

    /// True when the server refused for `reason`.
    pub fn is(&self, reason: ErrorReason) -> bool {
        self.reason() == Some(reason)
    }

    /// For a policy denial, what is still missing (votes, checks, owners).
    pub fn missing(&self) -> &[String] {
        match self {
            Error::Status { missing, .. } => missing,
            _ => &[],
        }
    }

    /// How long the server asked the caller to wait before retrying.
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Error::Status { retry_after, .. } => *retry_after,
            _ => None,
        }
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
        let details = Details::of(&err);
        Error::Status {
            code: err.code,
            message: err.message.unwrap_or_default(),
            reason: details.reason,
            missing: details.missing,
            retry_after: details.retry_after,
        }
    }
}

/// What an error's details say.
#[derive(Default)]
pub(crate) struct Details {
    pub(crate) reason: Option<ErrorReason>,
    pub(crate) missing: Vec<String>,
    pub(crate) retry_after: Option<std::time::Duration>,
}

impl Details {
    pub(crate) fn of(err: &connectrpc::ConnectError) -> Self {
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
        Details {
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

/// A reason as the wire names it: `ETAG_MISMATCH`.
pub fn reason_name(reason: ErrorReason) -> &'static str {
    let name = reason.proto_name();
    name.strip_prefix("ERROR_REASON_").unwrap_or(name)
}

impl Error {
    /// The error as a Connect error with the same code, reason, missing
    /// requirements and retry delay, for a service that relays it.
    pub fn to_connect(&self) -> connectrpc::ConnectError {
        let (code, reason, missing, retry_after) = match self {
            Error::Status {
                code,
                reason,
                missing,
                retry_after,
                ..
            } => (*code, *reason, missing.clone(), *retry_after),
            Error::Invalid(_) | Error::Convert(_) => (Code::InvalidArgument, None, vec![], None),
            Error::Conflict { .. } => (Code::FailedPrecondition, None, vec![], None),
            Error::Timeout(_) => (Code::DeadlineExceeded, None, vec![], None),
            _ => (Code::Internal, None, vec![], None),
        };
        let message = match self {
            Error::Status { message, .. } => message.clone(),
            other => other.to_string(),
        };
        let mut err = connectrpc::ConnectError::new(code, message);
        if let Some(reason) = reason {
            err = err.with_detail(connectrpc::ErrorDetail::from_message(
                "google.rpc.ErrorInfo",
                &rpc::ErrorInfo {
                    reason: reason_name(reason).into(),
                    domain: rill_proto::ERROR_DOMAIN.into(),
                    metadata: Default::default(),
                },
            ));
        }
        if !missing.is_empty() {
            err = err.with_detail(connectrpc::ErrorDetail::from_message(
                "google.rpc.PreconditionFailure",
                &rpc::PreconditionFailure {
                    violations: missing
                        .into_iter()
                        .map(|description| rpc::precondition_failure::Violation {
                            r#type: "POLICY".into(),
                            subject: String::new(),
                            description,
                        })
                        .collect(),
                },
            ));
        }
        if let Some(delay) = retry_after {
            err = err.with_detail(connectrpc::ErrorDetail::from_message(
                "google.rpc.RetryInfo",
                &rpc::RetryInfo {
                    retry_delay: buffa_types::google::protobuf::Duration {
                        seconds: delay.as_secs() as i64,
                        nanos: delay.subsec_nanos() as i32,
                        ..Default::default()
                    }
                    .into(),
                },
            ));
        }
        err
    }
}

/// Result alias used throughout the SDK.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(type_name: &str, message: &impl Message) -> connectrpc::ErrorDetail {
        connectrpc::ErrorDetail::from_message(type_name, message)
    }

    #[test]
    fn details_give_the_reason_what_is_missing_and_when_to_retry() {
        let err = connectrpc::ConnectError::new(Code::FailedPrecondition, "not yet")
            .with_detail(detail(
                "google.rpc.ErrorInfo",
                &rpc::ErrorInfo {
                    reason: "POLICY_DENIED".into(),
                    domain: rill_proto::ERROR_DOMAIN.into(),
                    metadata: Default::default(),
                },
            ))
            .with_detail(detail(
                "google.rpc.PreconditionFailure",
                &rpc::PreconditionFailure {
                    violations: vec![rpc::precondition_failure::Violation {
                        r#type: "POLICY".into(),
                        subject: String::new(),
                        description: "Review +2".into(),
                    }],
                },
            ))
            .with_detail(detail(
                "google.rpc.RetryInfo",
                &rpc::RetryInfo {
                    retry_delay: buffa_types::google::protobuf::Duration {
                        seconds: 3,
                        ..Default::default()
                    }
                    .into(),
                },
            ));
        let err = Error::from(err);
        assert!(err.is(ErrorReason::PolicyDenied));
        assert_eq!(err.missing(), ["Review +2"]);
        assert_eq!(err.retry_after(), Some(std::time::Duration::from_secs(3)));
        let relayed = Error::from(err.to_connect());
        assert!(relayed.is(ErrorReason::PolicyDenied));
        assert_eq!(relayed.missing(), ["Review +2"]);
        assert_eq!(
            relayed.retry_after(),
            Some(std::time::Duration::from_secs(3))
        );
    }

    #[test]
    fn a_reason_from_another_domain_is_not_ours() {
        let err = connectrpc::ConnectError::new(Code::NotFound, "gone").with_detail(detail(
            "google.rpc.ErrorInfo",
            &rpc::ErrorInfo {
                reason: "NOT_FOUND".into(),
                domain: "example.com".into(),
                metadata: Default::default(),
            },
        ));
        assert_eq!(Error::from(err).reason(), None);
    }

    #[test]
    fn status_mapping_preserves_code() {
        let err: Error = connectrpc::ConnectError::not_found("no such repo").into();
        assert_eq!(err.reason(), None, "no ErrorInfo, no reason");
        assert!(err.is_not_found());
        assert_eq!(err.code(), Some(Code::NotFound));
        assert_eq!(err.to_string(), "NotFound: no such repo");
    }
}
