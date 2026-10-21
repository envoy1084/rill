//! The CLI error type: what went wrong, how to say it, and which exit code
//! it maps to (see [`crate::exit`]).

use thiserror::Error;
use tonic::Code;

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

impl From<tonic::Status> for CliError {
    fn from(status: tonic::Status) -> Self {
        CliError::Sdk(rill_sdk::Error::from(status))
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
        Error::Status { code, message } => {
            let what = match code {
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
                _ => "server error",
            };
            if message.is_empty() {
                what.to_string()
            } else {
                format!("{what}: {message}")
            }
        }
        Error::Transport(e) => format!("cannot reach the server: {e}"),
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
    fn sdk_status_is_described_plainly() {
        let err = CliError::Sdk(rill_sdk::Error::Status {
            code: Code::NotFound,
            message: "no such repo".into(),
        });
        assert_eq!(err.to_string(), "not found: no such repo");
        assert_eq!(err.code(), "not_found");
        assert_eq!(CliError::usage("x").code(), "usage");
    }
}
