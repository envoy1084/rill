//! The `ErrorReason` each error carries: why it failed, for clients to
//! branch on.

use rill_proto::rill::v1::ErrorReason as R;
use tonic::Code;

use super::Error;

impl Error {
    /// The machine-readable reason for this error.
    pub fn reason(&self) -> R {
        match self {
            Error::Store(e) => store_reason(e).unwrap_or_else(|| by_code(self.code())),
            Error::Core(rill_core::Error::Store(e)) => {
                store_reason(e).unwrap_or_else(|| by_code(self.code()))
            }
            Error::Policy(e) => policy_reason(e).unwrap_or_else(|| by_code(self.code())),
            Error::Review(e) => {
                use rill_review::error::Error as E;
                match e {
                    E::Contended(_) => R::Contended,
                    E::InvalidStatus { .. } | E::ThreadAlreadyResolved(_) => R::InvalidState,
                    E::StaleVersion { .. } => R::InvalidState,
                    E::Policy(p) => policy_reason(p).unwrap_or_else(|| by_code(self.code())),
                    E::Store(s) => store_reason(s).unwrap_or_else(|| by_code(self.code())),
                    _ => by_code(self.code()),
                }
            }
            Error::Lander(e) => {
                use rill_lander::Error as E;
                match e {
                    E::QueueFull { .. } => R::QueueFull,
                    E::TooManyRequests { .. } => R::RateLimited,
                    E::AlreadyQueued { .. } => R::InvalidState,
                    E::RevertFailed { .. } | E::Fenced => R::Contended,
                    E::Store(s) => store_reason(s).unwrap_or_else(|| by_code(self.code())),
                    _ => by_code(self.code()),
                }
            }
            Error::Identity(rill_identity::Error::Policy(p)) => {
                policy_reason(p).unwrap_or_else(|| by_code(self.code()))
            }
            Error::Convert(_) | Error::Types(_) => R::InvalidField,
            Error::InvalidName(_) => R::InvalidName,
            Error::InvalidRevision(_) => R::InvalidRevision,
            Error::InvalidFilter(_) => R::InvalidFilter,
            Error::InvalidPageToken(_) => R::InvalidPageToken,
            Error::RequestIdReused => R::RequestIdReused,
            Error::EtagMismatch(_) => R::EtagMismatch,
            Error::PolicyDenied { .. } => R::PolicyDenied,
            Error::MissingObjects(_) => R::MissingObjects,
            Error::RateLimited { .. } => R::RateLimited,
            Error::Exhausted(_) => R::BudgetExhausted,
            _ => by_code(self.code()),
        }
    }
}

fn store_reason(e: &rill_store::Error) -> Option<R> {
    use rill_store::Error as E;
    match e {
        E::Conflict(_) => Some(R::Contended),
        E::Redacted { .. } => Some(R::Redacted),
        E::TooLarge(_) => Some(R::TooLarge),
        _ => None,
    }
}

fn policy_reason(e: &rill_policy::Error) -> Option<R> {
    use rill_policy::Error as E;
    match e {
        E::TokenRejected(_) | E::Widening(_) => Some(R::CapabilityDenied),
        _ => None,
    }
}

/// The reason an error has when nothing more specific is known.
fn by_code(code: Code) -> R {
    match code {
        Code::InvalidArgument | Code::OutOfRange => R::InvalidField,
        Code::NotFound => R::NotFound,
        Code::AlreadyExists => R::AlreadyExists,
        Code::PermissionDenied => R::PermissionDenied,
        Code::Unauthenticated => R::Unauthenticated,
        Code::FailedPrecondition => R::InvalidState,
        Code::Aborted => R::Contended,
        Code::ResourceExhausted => R::TooLarge,
        Code::Unimplemented => R::Unimplemented,
        Code::Unavailable | Code::Cancelled => R::Unavailable,
        Code::DeadlineExceeded => R::Timeout,
        _ => R::Internal,
    }
}
