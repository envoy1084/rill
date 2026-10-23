//! The CLI error type: what went wrong, how to say it, and which exit code
//! it maps to (see [`crate::exit`]).

use rill_sdk::{Code, ErrorReason};
use thiserror::Error;

/// Everything a command can fail with.
#[derive(Debug, Error)]
pub enum CliError {
    /// The command line was valid but the combination of options makes no
    /// sense here (for example a checkout command outside a checkout).
    #[error("{0}")]
    Usage(String),

    /// The operation completed but left conflicts, or a landing could not
    /// proceed.
    #[error("{0}")]
    Conflict(String),

    /// The SDK or server failed.
    #[error("{}", describe_sdk(.0))]
    Sdk(#[from] rill_sdk::Error),

    /// A value could not be parsed.
    #[error("invalid value: {0}")]
    Invalid(#[from] rill_types::Error),

    /// A local filesystem operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A git import failed.
    #[error("import error: {0}")]
    Import(#[from] rill_git_import::Error),

    /// Local storage failed (import runs directly against it).
    #[error("storage error: {0}")]
    Store(#[from] rill_store::Error),

    /// The service failed to start or stopped with an error.
    #[error("server error: {0}")]
    Server(#[from] rill_api::Error),

    /// The cache tier failed to start or stopped with an error.
    #[error("cache error: {0}")]
    Cache(#[from] rill_cache::Error),

    /// A JSON value could not be written.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Anything else.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CliError {
    /// A usage error.
    pub fn usage(msg: impl Into<String>) -> Self {
        CliError::Usage(msg.into())
    }

    /// A conflict error.
    pub fn conflict(msg: impl Into<String>) -> Self {
        CliError::Conflict(msg.into())
    }

    /// The stable error code used in `--json` error output.
    pub fn code(&self) -> String {
        match self {
            CliError::Usage(_) => "usage".into(),
            CliError::Conflict(_) => "conflict".into(),
            CliError::Sdk(e) => rill_mcp::error::sdk_code(e),
            CliError::Invalid(_) => "invalid".into(),
            CliError::Io(_) => "io".into(),
            CliError::Import(_) => "import".into(),
            CliError::Store(_) => "storage".into(),
            CliError::Server(_) => "server".into(),
            CliError::Cache(_) => "cache".into(),
            CliError::Json(_) => "json".into(),
            CliError::Other(_) => "error".into(),
        }
    }
}

impl CliError {
    /// The server's reason, when the server refused.
    pub fn reason(&self) -> Option<ErrorReason> {
        match self {
            CliError::Sdk(e) => e.reason(),
            _ => None,
        }
    }

    /// The error as `--json` prints it.
    pub fn to_json(&self) -> serde_json::Value {
        let (missing, retry_after) = match self {
            CliError::Sdk(e) => (e.missing().to_vec(), e.retry_after()),
            _ => (Vec::new(), None),
        };
        serde_json::json!({
            "code": self.code(),
            "reason": self.reason().map(rill_sdk::error::reason_name),
            "message": self.to_string(),
            "missing": missing,
            "retry_after_ms": retry_after.map(|d| d.as_millis() as u64),
            "exit": crate::exit::code_for(self),
        })
    }

    /// What to do about the error, a line each, for people.
    pub fn hints(&self) -> Vec<String> {
        let CliError::Sdk(e) = self else {
            return Vec::new();
        };
        let mut hints: Vec<String> = e.missing().iter().map(|m| format!("needs: {m}")).collect();
        if let Some(wait) = e.retry_after() {
            hints.push(format!("retry in {}s", wait.as_secs().max(1)));
        }
        let advice = match e.reason() {
            Some(ErrorReason::Unauthenticated) => {
                Some("sign in with `rill login`, or pass --token")
            }
            Some(ErrorReason::EtagMismatch) | Some(ErrorReason::RefMoved) => {
                Some("it changed since it was read: read it again, then retry")
            }
            Some(ErrorReason::RequestIdReused) => {
                Some("that request id was used for another request: use a fresh one")
            }
            Some(ErrorReason::ConflictsUnresolved) => {
                Some("resolve the conflicts first: `rill conflicts`, then `rill resolve`")
            }
            Some(ErrorReason::VersionSuperseded) => {
                Some("the change has a newer version: act on that one")
            }
            _ => None,
        };
        hints.extend(advice.map(str::to_string));
        hints
    }
}

impl From<connectrpc::ConnectError> for CliError {
    fn from(err: connectrpc::ConnectError) -> Self {
        CliError::Sdk(rill_sdk::Error::from(err))
    }
}

impl From<rill_mcp::ToolError> for CliError {
    fn from(e: rill_mcp::ToolError) -> Self {
        match e {
            rill_mcp::ToolError::Sdk(e) => CliError::Sdk(e),
            other => CliError::Usage(other.to_string()),
        }
    }
}

/// Short, human phrasing of an SDK error. Status codes become plain words
/// so `rill` never prints `NotFound:` at a user.
fn describe_sdk(e: &rill_sdk::Error) -> String {
    use rill_sdk::Error;
    match e {
        Error::Status {
            code,
            message,
            reason,
            ..
        } => {
            let by_reason = match reason {
                Some(ErrorReason::PolicyDenied) => Some("not allowed by policy"),
                Some(ErrorReason::CapabilityDenied) => Some("outside your token's scope"),
                Some(ErrorReason::EtagMismatch) | Some(ErrorReason::RefMoved) => {
                    Some("changed meanwhile")
                }
                Some(ErrorReason::RateLimited) => Some("rate limited"),
                Some(ErrorReason::QueueFull) => Some("the landing queue is full"),
                Some(ErrorReason::BudgetExhausted) => Some("budget exhausted"),
                Some(ErrorReason::InvalidName) => Some("invalid name"),
                Some(ErrorReason::InvalidRevision) => Some("invalid revision"),
                Some(ErrorReason::InvalidFilter) => Some("invalid filter"),
                Some(ErrorReason::MissingObjects) => Some("objects missing"),
                Some(ErrorReason::Redacted) => Some("redacted"),
                _ => None,
            };
            let what = by_reason.unwrap_or(match code {
                Code::NotFound => "not found",
                Code::AlreadyExists => "already exists",
                Code::PermissionDenied => "permission denied",
                Code::Unauthenticated => "unauthenticated",
                Code::InvalidArgument => "invalid argument",
                Code::FailedPrecondition => "precondition failed",
                Code::Aborted => "aborted",
                Code::Unavailable => "server unavailable",
                Code::Unimplemented => "not supported by the server",
                Code::DeadlineExceeded => "timed out",
                Code::ResourceExhausted => "too much",
                _ => "server error",
            });
            if message.is_empty() {
                what.to_string()
            } else {
                format!("{what}: {message}")
            }
        }
        Error::Config(m) => format!("configuration: {m}"),
        Error::Checkout(m) => format!("checkout: {m}"),
        Error::Conflict { paths, .. } => format!(
            "conflicts in {}",
            paths
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Error::LandFailed {
            status, message, ..
        } => format!("landing {status:?}: {message}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_errors_carry_the_reason_and_what_is_missing() {
        let err = CliError::Sdk(rill_sdk::Error::Status {
            code: Code::FailedPrecondition,
            message: "submit policy does not allow landing".into(),
            reason: Some(ErrorReason::PolicyDenied),
            missing: vec!["Review +2".into()],
            retry_after: None,
        });
        let doc = err.to_json();
        assert_eq!(doc["reason"], "POLICY_DENIED");
        assert_eq!(doc["missing"][0], "Review +2");
        assert_eq!(doc["exit"], crate::exit::DENIED);
        assert_eq!(
            err.to_string(),
            "not allowed by policy: submit policy does not allow landing"
        );
        assert_eq!(err.hints(), ["needs: Review +2"]);
    }

    #[test]
    fn sdk_status_is_described_plainly() {
        let err = CliError::Sdk(rill_sdk::Error::status(Code::NotFound, "no such repo"));
        assert_eq!(err.to_string(), "not found: no such repo");
        assert_eq!(err.code(), "not_found");
        assert_eq!(CliError::usage("x").code(), "usage");
    }
}
