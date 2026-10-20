use rill_types::ChangeVersionHash;
use thiserror::Error;

use crate::reader::GitSha;

/// Errors produced while reading git data or importing it.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    #[error(transparent)]
    Core(#[from] rill_core::Error),

    #[error(transparent)]
    Types(#[from] rill_types::Error),

    #[error("git: {0}")]
    Git(String),

    #[error("unknown git ref or object: {0}")]
    UnknownRef(String),

    #[error("git commit {0} not found")]
    CommitNotFound(GitSha),

    #[error("parent {parent} of {commit} was not imported")]
    MissingParent { commit: GitSha, parent: GitSha },

    #[error("invalid git path {0:?}: {1}")]
    InvalidPath(String, rill_types::Error),

    #[error("trunk head {trunk:?} is not the first parent of {next}; refusing to land")]
    TrunkDiverged {
        trunk: Option<ChangeVersionHash>,
        next: ChangeVersionHash,
    },

    #[error("import map codec error: {0}")]
    Codec(String),

    #[error("reader stopped unexpectedly")]
    ReaderStopped,

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl Error {
    /// Wrap any gix error.
    pub fn git(e: impl std::fmt::Display) -> Self {
        Error::Git(e.to_string())
    }
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
