use thiserror::Error;

/// Errors produced while converting wire messages to `rill-types` values.
///
/// Every variant names the field that failed so a client can be told which
/// part of its request was malformed.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConvertError {
    #[error("field {field}: expected {expected} bytes, got {actual}")]
    BadLength {
        field: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("field {field}: required but missing")]
    Missing { field: &'static str },

    #[error("field {field}: unknown or unspecified enum value {value}")]
    BadEnum { field: &'static str, value: i32 },

    #[error("field {field}: {source}")]
    Invalid {
        field: &'static str,
        #[source]
        source: rill_types::Error,
    },

    #[error("field {field}: value {value} out of range")]
    OutOfRange { field: &'static str, value: i64 },
}

impl ConvertError {
    /// Wrap a `rill-types` validation error with the offending field.
    pub fn invalid(field: &'static str, source: rill_types::Error) -> Self {
        ConvertError::Invalid { field, source }
    }

    /// A required message field that was not set.
    pub fn missing(field: &'static str) -> Self {
        ConvertError::Missing { field }
    }
}

impl From<ConvertError> for tonic::Status {
    fn from(err: ConvertError) -> Self {
        tonic::Status::invalid_argument(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ConvertError>;
