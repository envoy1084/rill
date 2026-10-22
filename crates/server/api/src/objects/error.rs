//! HTTP-level errors of the object protocol and their responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rill_runtime::error::Error;
use tonic::Code;

/// An HTTP-level error.
#[derive(Debug)]
pub struct ObjectError {
    pub(super) status: StatusCode,
    pub(super) message: String,
}

impl ObjectError {
    /// A 400.
    pub fn bad_request(message: impl Into<String>) -> Self {
        ObjectError {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    /// A 404.
    pub fn not_found(message: impl Into<String>) -> Self {
        ObjectError {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl From<Error> for ObjectError {
    fn from(err: Error) -> Self {
        if err.is_redacted() {
            return ObjectError {
                status: StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
                message: err.to_string(),
            };
        }
        let status = match err.code() {
            Code::NotFound => StatusCode::NOT_FOUND,
            Code::PermissionDenied => StatusCode::FORBIDDEN,
            Code::Unauthenticated => StatusCode::UNAUTHORIZED,
            Code::InvalidArgument | Code::FailedPrecondition => StatusCode::BAD_REQUEST,
            Code::AlreadyExists | Code::Aborted => StatusCode::CONFLICT,
            Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ObjectError {
            status,
            message: err.to_string(),
        }
    }
}

impl From<rill_store::Error> for ObjectError {
    fn from(err: rill_store::Error) -> Self {
        Error::from(err).into()
    }
}

/// Render an error as a plain-text response.
pub fn error_response(err: ObjectError) -> Response {
    if err.status.is_server_error() {
        tracing::error!(error = %err.message, "object protocol error");
    }
    (err.status, err.message).into_response()
}
