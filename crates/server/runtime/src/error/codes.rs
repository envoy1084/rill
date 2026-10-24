//! The status code each error maps to, from its meaning rather than the
//! crate that produced it.

use tonic::Code;

use super::Error;

impl Error {
    /// The status code this error maps to.
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
            Error::Jobs(rill_jobs::Error::Foreign(_)) => Code::InvalidArgument,
            Error::Exec(e) => exec_code(e),
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
                rill_search::Error::InvalidPattern(_)
                | rill_search::Error::InvalidGlob(_)
                | rill_search::Error::InvalidScip(_) => Code::InvalidArgument,
                rill_search::Error::RootNotIndexed(_) => Code::Unavailable,
                rill_search::Error::Store(s) => store_code(s),
                rill_search::Error::Core(c) => core_code(c),
                rill_search::Error::Types(_) => Code::InvalidArgument,
                rill_search::Error::Codec(_)
                | rill_search::Error::Join(_)
                | rill_search::Error::Worker => Code::Internal,
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
            Error::InvalidName(_)
            | Error::InvalidRevision(_)
            | Error::InvalidFilter(_)
            | Error::InvalidPageToken(_)
            | Error::RequestIdReused => Code::InvalidArgument,
            Error::EtagMismatch(_) => Code::Aborted,
            Error::PolicyDenied { .. } | Error::MissingObjects(_) => Code::FailedPrecondition,
        }
    }
}

pub(super) fn exec_code(e: &rill_exec::Error) -> Code {
    use rill_exec::Error as E;
    match e {
        E::InvalidDigest(_) | E::DigestMismatch(_) | E::InvalidAction(_) => Code::InvalidArgument,
        E::MissingBlob(_) => Code::FailedPrecondition,
        E::Contended(_) => Code::Unavailable,
        E::Jobs(rill_jobs::Error::Lost(_)) => Code::Aborted,
        E::Store(e) => store_code(e),
        _ => Code::Internal,
    }
}

pub(super) fn store_code(e: &rill_store::Error) -> Code {
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

pub(super) fn core_code(e: &rill_core::Error) -> Code {
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

pub(super) fn policy_code(e: &rill_policy::Error) -> Code {
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

pub(super) fn review_code(e: &rill_review::error::Error) -> Code {
    use rill_review::error::Error as E;
    match e {
        E::ReviewNotFound(_)
        | E::NoVersions(_)
        | E::VersionNotFound(_)
        | E::ThreadNotFound { .. } => Code::NotFound,
        E::AlreadySubmitted(_) => Code::AlreadyExists,
        E::InvalidStatus { .. } | E::ThreadAlreadyResolved(_) | E::StaleVersion { .. } => {
            Code::FailedPrecondition
        }
        E::WrongChange { .. }
        | E::BadResult(_)
        | E::UnknownLabel(_)
        | E::VoteOutOfRange { .. }
        | E::InvalidStack(_)
        | E::Types(_) => Code::InvalidArgument,
        E::MaxRequiresHuman(_) => Code::PermissionDenied,
        E::Contended(_) => Code::Aborted,
        E::Policy(p) => policy_code(p),
        E::Core(c) => core_code(c),
        E::Store(s) => store_code(s),
        E::Join(_) => Code::Internal,
    }
}

pub(super) fn lander_code(e: &rill_lander::Error) -> Code {
    use rill_lander::Error as E;
    match e {
        E::NotLanded(_) | E::ChangeNotFound(_) | E::UnknownTicket(_) => Code::NotFound,
        E::Shutdown => Code::Unavailable,
        E::RevertFailed { .. } | E::Fenced => Code::Aborted,
        E::TooManyRequests { .. } | E::QueueFull { .. } => Code::ResourceExhausted,
        E::Types(_) | E::Config(_) | E::GroupSize { .. } => Code::InvalidArgument,
        E::AlreadyQueued { .. } => Code::FailedPrecondition,
        E::Core(c) => core_code(c),
        E::Store(s) | E::Jobs(rill_jobs::Error::Store(s)) => store_code(s),
        E::Jobs(rill_jobs::Error::Lost(_)) => Code::Aborted,
        E::Jobs(rill_jobs::Error::Foreign(_)) => Code::Internal,
        E::Hook(_) | E::Verifier(_) | E::Codec(_) | E::Join(_) => Code::Internal,
    }
}
