use rill_types::{ChangeVersionHash, ObjectHash, RepoPathBuf};
use thiserror::Error;

/// Errors produced by the core algorithms.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    #[error(transparent)]
    Types(#[from] rill_types::Error),

    #[error("change version not found: {0}")]
    ChangeNotFound(ChangeVersionHash),

    #[error("tree not found: {0}")]
    TreeNotFound(ObjectHash),

    #[error("path not found: {0:?}")]
    PathNotFound(RepoPathBuf),

    #[error("not a directory: {0:?}")]
    NotADirectory(RepoPathBuf),

    #[error("not a file: {0:?}")]
    NotAFile(RepoPathBuf),

    #[error("binary content at {0:?}")]
    BinaryContent(RepoPathBuf),

    #[error("invalid change: {0}")]
    InvalidChange(String),

    #[error("content size mismatch: expected {expected} bytes, got {actual}")]
    ContentSize { expected: u64, actual: u64 },

    #[error("upload interrupted: {0}")]
    Upload(String),

    #[error("invalid patch: {0}")]
    InvalidPatch(String),

    #[error("codec error: {0}")]
    Codec(String),

    #[error("derivation of {kind} for {hash} failed: {reason}")]
    Derive {
        kind: &'static str,
        hash: ObjectHash,
        reason: String,
    },

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Whether the content asked for was redacted.
    pub fn is_redacted(&self) -> bool {
        matches!(self, Error::Store(rill_store::Error::Redacted { .. }))
    }
}
