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
//!
//! Scripts and agents can rely on these: `3` means "look at the output
//! and decide", `1` means "something is broken", `2` means "fix the
//! invocation".

use rill_sdk::Code;

use crate::error::CliError;

/// Success.
pub const OK: i32 = 0;
/// Failure.
pub const ERROR: i32 = 1;
/// Bad invocation.
pub const USAGE: i32 = 2;
/// Conflict or not landable.
pub const CONFLICT: i32 = 3;

/// The exit code for an error.
pub fn code_for(error: &CliError) -> i32 {
    match error {
        CliError::Usage(_) => USAGE,
        CliError::Conflict(_) => CONFLICT,
        CliError::Sdk(e) => match e {
            rill_sdk::Error::Conflict { .. } | rill_sdk::Error::LandFailed { .. } => CONFLICT,
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
        let aborted = CliError::Sdk(rill_sdk::Error::Status {
            code: Code::Aborted,
            message: String::new(),
        });
        assert_eq!(code_for(&aborted), CONFLICT);
        let missing = CliError::Sdk(rill_sdk::Error::Status {
            code: Code::NotFound,
            message: String::new(),
        });
        assert_eq!(code_for(&missing), ERROR);
        assert_eq!(
            code_for(&CliError::Sdk(rill_sdk::Error::Timeout(
                std::time::Duration::from_secs(1)
            ))),
            ERROR
        );
    }
}
