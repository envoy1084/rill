//! Server error type and its mapping onto gRPC statuses.
//!
//! Every lower crate's error converts into [`Error`] via `From`, and
//! [`Error`] converts into [`tonic::Status`] so handlers can use `?` all
//! the way down. The mapping picks the status code from the *meaning* of
//! the underlying error (not found, conflict, bad input) rather than from
//! which crate produced it.

use rill_proto::ConvertError;
use thiserror::Error;
use tonic::{Code, Status};

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

    /// The gRPC code this error maps to.
    pub fn code(&self) -> Code {
        match self {
            Error::Store(e) => store_code(e),
            Error::Core(e) => core_code(e),
            Error::Policy(e) => policy_code(e),
            Error::Review(e) => review_code(e),
            Error::Lander(e) => lander_code(e),
            Error::Events(rill_events::Error::Store(e)) => store_code(e),
            Error::Jobs(rill_jobs::Error::Store(e)) => store_code(e),
            Error::Jobs(rill_jobs::Error::Lost(_)) => Code::Aborted,
            Error::Exec(_) => Code::Internal,
            Error::Identity(e) => match e {
                rill_identity::Error::Store(e) => store_code(e),
                rill_identity::Error::Policy(e) => policy_code(e),
                rill_identity::Error::KeyService { .. } => Code::Unavailable,
                rill_identity::Error::Oidc(_) | rill_identity::Error::Saml(_) => {
                    Code::Unauthenticated
                }
                rill_identity::Error::Denied(_) => Code::PermissionDenied,
                rill_identity::Error::Config(_) => Code::Internal,
            },
            Error::Search(e) => match e {
                rill_search::Error::InvalidPattern(_) | rill_search::Error::InvalidGlob(_) => {
                    Code::InvalidArgument
                }
                rill_search::Error::RootNotIndexed(_) => Code::Unavailable,
                rill_search::Error::Store(s) => store_code(s),
                rill_search::Error::Core(c) => core_code(c),
                rill_search::Error::Types(_) => Code::InvalidArgument,
                rill_search::Error::Codec(_) | rill_search::Error::Join(_) => Code::Internal,
            },
            Error::Types(_) | Error::Convert(_) | Error::InvalidArgument(_) => {
                Code::InvalidArgument
            }
            Error::Io(_) | Error::Transport(_) => Code::Unavailable,
            Error::Config(_) | Error::Internal(_) => Code::Internal,
            Error::NotFound(_) => Code::NotFound,
            Error::AlreadyExists(_) => Code::AlreadyExists,
            Error::FailedPrecondition(_) => Code::FailedPrecondition,
            Error::PermissionDenied(_) => Code::PermissionDenied,
            Error::Unauthenticated(_) => Code::Unauthenticated,
            Error::Unimplemented(_) => Code::Unimplemented,
            Error::Unavailable(_) => Code::Unavailable,
            Error::Exhausted(_) | Error::RateLimited { .. } => Code::ResourceExhausted,
        }
    }
}

fn store_code(e: &rill_store::Error) -> Code {
    use rill_store::Error as E;
    match e {
        E::ObjectNotFound { .. } | E::KeyNotFound(_) | E::NotFound(_) => Code::NotFound,
        E::Conflict(_) => Code::Aborted,
        E::HashMismatch { .. } | E::Types(_) | E::Invalid(_) => Code::InvalidArgument,
        E::Codec(_) | E::Corrupt { .. } => Code::DataLoss,
        E::IncompatibleFormat { .. } | E::Redacted { .. } => Code::FailedPrecondition,
        E::TooLarge(_) => Code::ResourceExhausted,
        E::Sealed(_) => Code::Internal,
        E::Backend(_) | E::Io(_) | E::Retryable(_) | E::CommitUnknown(_) => Code::Unavailable,
    }
}

fn core_code(e: &rill_core::Error) -> Code {
    use rill_core::Error as E;
    match e {
        E::Store(s) => store_code(s),
        E::ChangeNotFound(_) | E::TreeNotFound(_) | E::PathNotFound(_) => Code::NotFound,
        E::NotADirectory(_)
        | E::NotAFile(_)
        | E::BinaryContent(_)
        | E::InvalidChange(_)
        | E::Upload(_)
        | E::InvalidPatch(_)
        | E::Types(_) => Code::InvalidArgument,
        E::ContentSize { .. } | E::Codec(_) => Code::DataLoss,
        E::Derive { .. } | E::Join(_) => Code::Internal,
    }
}

fn policy_code(e: &rill_policy::Error) -> Code {
    use rill_policy::Error as E;
    match e {
        E::Store(s) => store_code(s),
        E::Token(_) | E::Unauthenticated(_) | E::TokenExpired { .. } => Code::Unauthenticated,
        E::TokenRejected(_) => Code::PermissionDenied,
        E::PolicyEvaluation(_) => Code::Internal,
        E::InvalidGlob { .. }
        | E::PolicyParse(_)
        | E::PolicyValidation(_)
        | E::Widening(_)
        | E::InvalidKey(_)
        | E::Types(_) => Code::InvalidArgument,
    }
}

fn review_code(e: &rill_review::error::Error) -> Code {
    use rill_review::error::Error as E;
    match e {
        E::ReviewNotFound(_)
        | E::NoVersions(_)
        | E::VersionNotFound(_)
        | E::ThreadNotFound { .. } => Code::NotFound,
        E::AlreadySubmitted(_) => Code::AlreadyExists,
        E::InvalidStatus { .. } | E::ThreadAlreadyResolved(_) => Code::FailedPrecondition,
        E::WrongChange { .. }
        | E::UnknownLabel(_)
        | E::VoteOutOfRange { .. }
        | E::InvalidStack(_)
        | E::Types(_) => Code::InvalidArgument,
        E::MaxRequiresHuman(_) => Code::PermissionDenied,
        E::Contended(_) => Code::Aborted,
        E::ReviewerTimeout(_) => Code::DeadlineExceeded,
        E::Policy(p) => policy_code(p),
        E::Core(c) => core_code(c),
        E::Store(s) => store_code(s),
        E::ReviewerFailed { .. } | E::Join(_) => Code::Internal,
    }
}

fn lander_code(e: &rill_lander::Error) -> Code {
    use rill_lander::Error as E;
    match e {
        E::NotLanded(_) | E::ChangeNotFound(_) | E::UnknownTicket(_) => Code::NotFound,
        E::Shutdown => Code::Unavailable,
        E::RevertFailed { .. } | E::Fenced => Code::Aborted,
        E::TooManyRequests { .. } | E::QueueFull { .. } => Code::ResourceExhausted,
        E::Types(_) | E::Config(_) => Code::InvalidArgument,
        E::Core(c) => core_code(c),
        E::Store(s) | E::Jobs(rill_jobs::Error::Store(s)) => store_code(s),
        E::Jobs(rill_jobs::Error::Lost(_)) => Code::Aborted,
        E::Hook(_) | E::Verifier(_) | E::Codec(_) | E::Join(_) => Code::Internal,
    }
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        let code = err.code();
        if code == Code::Internal {
            tracing::error!(error = %err, "internal error in handler");
        }
        let retry_after = match &err {
            Error::Lander(rill_lander::Error::QueueFull {
                retry_after_secs, ..
            })
            | Error::RateLimited {
                retry_after_secs, ..
            } => Some(*retry_after_secs),
            _ => None,
        };
        let mut status = Status::new(code, err.to_string());
        if let Some(secs) = retry_after {
            status.metadata_mut().insert(
                "retry-after",
                secs.to_string().parse().expect("digits are ASCII"),
            );
        }
        status
    }
}

impl From<Status> for Error {
    fn from(status: Status) -> Self {
        let msg = status.message().to_string();
        match status.code() {
            Code::NotFound => Error::NotFound(msg),
            Code::PermissionDenied => Error::PermissionDenied(msg),
            Code::Unauthenticated => Error::Unauthenticated(msg),
            Code::InvalidArgument => Error::InvalidArgument(msg),
            Code::FailedPrecondition => Error::FailedPrecondition(msg),
            Code::AlreadyExists => Error::AlreadyExists(msg),
            Code::Unimplemented => Error::Unimplemented(msg),
            Code::Unavailable => Error::Unavailable(msg),
            _ => Error::Internal(msg),
        }
    }
}

#[cfg(test)]
mod tests {
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
