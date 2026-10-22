//! Error types: tool failures (reported to the agent as `isError`
//! results) and server failures (which end the stdio session).

use thiserror::Error;

/// Why a tool call failed. Every variant maps to a stable `code` string in
/// the structured error the agent receives.
#[derive(Debug, Error)]
pub enum ToolError {
    /// The arguments were missing or malformed.
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    /// No repository was given and none is configured.
    #[error("no repository: pass `repo` or set RILL_REPO")]
    NoRepo,

    /// The SDK or server failed.
    #[error(transparent)]
    Sdk(#[from] rill_sdk::Error),

    /// Anything else.
    #[error("{0}")]
    Other(String),
}

impl ToolError {
    /// Build an invalid-arguments error.
    pub fn invalid(msg: impl Into<String>) -> Self {
        ToolError::InvalidArgs(msg.into())
    }

    /// A stable, machine-readable code for the error.
    pub fn code(&self) -> String {
        match self {
            ToolError::InvalidArgs(_) => "invalid_arguments".into(),
            ToolError::NoRepo => "no_repo".into(),
            ToolError::Sdk(e) => sdk_code(e),
            ToolError::Other(_) => "error".into(),
        }
    }
}

impl From<rill_types::Error> for ToolError {
    fn from(e: rill_types::Error) -> Self {
        ToolError::InvalidArgs(e.to_string())
    }
}

/// The error code string for an SDK error: the lowercase gRPC code for a
/// status (`not_found`, `aborted`, ...) or the variant name otherwise.
pub fn sdk_code(e: &rill_sdk::Error) -> String {
    use rill_sdk::Error;
    match e {
        Error::Status { code, .. } => {
            let name = format!("{code:?}");
            let mut out = String::with_capacity(name.len() + 4);
            for (i, c) in name.chars().enumerate() {
                if c.is_ascii_uppercase() && i > 0 {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            }
            out
        }
        Error::Convert(_) => "convert".into(),
        Error::Invalid(_) => "invalid".into(),
        Error::Config(_) => "config".into(),
        Error::Io(_) => "io".into(),
        Error::Timeout(_) => "timeout".into(),
        Error::Conflict { .. } => "conflict".into(),
        Error::Checkout(_) => "checkout".into(),
        Error::LandFailed { .. } => "land_failed".into(),
    }
}

/// Result alias for tool handlers.
pub type ToolResult<T> = std::result::Result<T, ToolError>;

/// Why the stdio server stopped.
#[derive(Debug, Error)]
pub enum Error {
    /// Reading or writing the transport failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The client could not be configured.
    #[error("configuration error: {0}")]
    Config(String),

    /// The SDK failed outside a tool call.
    #[error(transparent)]
    Sdk(#[from] rill_sdk::Error),
}

/// Result alias for the server.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_are_snake_case() {
        let err = rill_sdk::Error::status(rill_sdk::Code::FailedPrecondition, "x");
        assert_eq!(sdk_code(&err), "failed_precondition");
        assert_eq!(ToolError::NoRepo.code(), "no_repo");
        assert_eq!(ToolError::from(err).code(), "failed_precondition");
    }
}
