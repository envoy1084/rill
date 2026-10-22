use thiserror::Error;

/// Errors produced while constructing or decoding core objects.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("invalid hash: {0}")]
    InvalidHash(String),

    #[error("invalid identifier: {0}")]
    InvalidId(String),

    #[error("invalid repository path {path:?}: {reason}")]
    InvalidPath { path: String, reason: &'static str },

    #[error("invalid ref name {0:?}")]
    InvalidRefName(String),

    #[error("unsupported schema {found} for {kind} (this build reads {expected})")]
    UnsupportedSchema {
        kind: &'static str,
        found: u16,
        expected: u16,
    },

    #[error("object kind mismatch: expected {expected:?}, found {found:?}")]
    KindMismatch { expected: String, found: String },

    #[error("change rejected: {0}")]
    ChangeRejected(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("encode error: {0}")]
    Encode(String),

    #[error("invalid merge term: {0}")]
    InvalidMerge(String),

    #[error("invalid object: {0}")]
    InvalidObject(String),
}

pub type Result<T> = std::result::Result<T, Error>;
