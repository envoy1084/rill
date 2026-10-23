//! Process exit codes.
//!
//! * `0` – success.
//! * `1` – the command failed: server error, not found, permission denied,
//!   transport failure, bad local state.
//! * `2` – usage: the command line could not be parsed, or the command is
//!   meaningless in this context (for example `rill status` outside a
//!   checkout, or `rill import` with an endpoint).
//! * `3` – conflict / not landable: the operation ran but the result needs
//!   attention: a sync or rebase left conflict markers, a land request was
//!   refused or failed, a submit evaluation said no, a compare-and-swap
//!   lost the race (`Aborted` / `FailedPrecondition` from the server).
//! * `4` – denied: the caller may not do this (unauthenticated, a missing
//!   permission or capability, the policy refuses).
//! * `5` – retryable: the server is unavailable or asked for a wait (a
//!   rate limit, a full queue); the same command may succeed later.
//!
//! Scripts and agents can rely on these: `3` means "look at the output
//! and decide", `4` "get permission or approval", `5` "try again later",
//! `1` "something is broken", `2` "fix the invocation".

use rill_sdk::{Code, ErrorReason};

use crate::error::CliError;

/// Success.
pub const OK: i32 = 0;
/// Failure.
pub const ERROR: i32 = 1;
/// Bad invocation.
pub const USAGE: i32 = 2;
/// Conflict or not landable.
pub const CONFLICT: i32 = 3;
/// Denied.
pub const DENIED: i32 = 4;
/// Retryable.
pub const RETRYABLE: i32 = 5;

/// The exit code for an error.
pub fn code_for(error: &CliError) -> i32 {
    match error {
        CliError::Usage(_) => USAGE,
        CliError::Conflict(_) => CONFLICT,
        CliError::Sdk(e) => match e {
            rill_sdk::Error::Conflict { .. } | rill_sdk::Error::LandFailed { .. } => CONFLICT,
            rill_sdk::Error::Status { .. } if e.is(ErrorReason::PolicyDenied) => DENIED,
            rill_sdk::Error::Status {
                code: Code::PermissionDenied | Code::Unauthenticated,
                ..
            } => DENIED,
            rill_sdk::Error::Status {
                code: Code::Unavailable | Code::DeadlineExceeded,
                ..
            } => RETRYABLE,
            rill_sdk::Error::Status {
                code: Code::ResourceExhausted,
                retry_after: Some(_),
                ..
            } => RETRYABLE,
            rill_sdk::Error::Timeout(_) => RETRYABLE,
            rill_sdk::Error::Status {
                code: Code::Aborted | Code::FailedPrecondition,
                ..
            } => CONFLICT,
            rill_sdk::Error::Config(_) => USAGE,
            _ => ERROR,
        },
        CliError::Invalid(_) => USAGE,
        _ => ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_errors_to_codes() {
        assert_eq!(code_for(&CliError::usage("x")), USAGE);
        assert_eq!(code_for(&CliError::conflict("x")), CONFLICT);
        let aborted = CliError::Sdk(rill_sdk::Error::status(Code::Aborted, String::new()));
        assert_eq!(code_for(&aborted), CONFLICT);
        let missing = CliError::Sdk(rill_sdk::Error::status(Code::NotFound, String::new()));
        assert_eq!(code_for(&missing), ERROR);
        assert_eq!(
            code_for(&CliError::Sdk(rill_sdk::Error::Timeout(
                std::time::Duration::from_secs(1)
            ))),
            RETRYABLE
        );
        let denied = CliError::Sdk(rill_sdk::Error::status(
            Code::PermissionDenied,
            String::new(),
        ));
        assert_eq!(code_for(&denied), DENIED);
        let policy = CliError::Sdk(rill_sdk::Error::Status {
            code: Code::FailedPrecondition,
            message: String::new(),
            reason: Some(ErrorReason::PolicyDenied),
            missing: vec!["Review +2".into()],
            retry_after: None,
        });
        assert_eq!(
            code_for(&policy),
            DENIED,
            "a policy refusal is not a conflict"
        );
        let limited = CliError::Sdk(rill_sdk::Error::Status {
            code: Code::ResourceExhausted,
            message: String::new(),
            reason: Some(ErrorReason::RateLimited),
            missing: vec![],
            retry_after: Some(std::time::Duration::from_secs(2)),
        });
        assert_eq!(code_for(&limited), RETRYABLE);
        let too_large = CliError::Sdk(rill_sdk::Error::status(
            Code::ResourceExhausted,
            String::new(),
        ));
        assert_eq!(code_for(&too_large), ERROR);
    }
}
