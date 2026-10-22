use std::path::PathBuf;

use thiserror::Error;

/// Errors produced by the verification runner.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    #[error(transparent)]
    Core(#[from] rill_core::Error),

    #[error(transparent)]
    Types(#[from] rill_types::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("invalid action: {0}")]
    InvalidAction(String),

    #[error("sandbox {backend} is not supported: {reason}")]
    NotSupported {
        backend: &'static str,
        reason: String,
    },

    #[error("sandbox setup failed: {0}")]
    Sandbox(String),

    #[error("failed to spawn {command:?}: {source}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("path escapes the sandbox root: {0}")]
    PathEscape(PathBuf),

    #[error("invalid verification config at {path}: {reason}")]
    Config { path: String, reason: String },

    #[error("codec error: {0}")]
    Codec(String),

    #[error(transparent)]
    Jobs(#[from] rill_jobs::Error),

    #[error("invalid digest {0}")]
    InvalidDigest(String),

    #[error("content does not match its digest {0}")]
    DigestMismatch(String),

    #[error("blob {0} is not in the CAS")]
    MissingBlob(String),

    #[error("contended: {0}")]
    Contended(String),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
