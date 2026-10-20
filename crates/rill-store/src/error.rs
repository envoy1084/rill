use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("object not found: {kind} {hash}")]
    ObjectNotFound {
        kind: rill_types::ObjectKind,
        hash: rill_types::ObjectHash,
    },

    #[error("object hash mismatch: expected {expected}, computed {actual}")]
    HashMismatch {
        expected: rill_types::ObjectHash,
        actual: rill_types::ObjectHash,
    },

    #[error("key not found: {0}")]
    KeyNotFound(String),

    #[error("transaction conflict: {0}")]
    Conflict(String),

    #[error("record not found: {0}")]
    NotFound(String),

    #[error("codec error: {0}")]
    Codec(String),

    #[error(
        "store format {found} does not match this build's format {expected}; \
         Rill is pre-release, so recreate the store"
    )]
    IncompatibleFormat { found: u32, expected: u32 },

    #[error("backend error: {0}")]
    Backend(String),

    #[error(transparent)]
    Types(#[from] rill_types::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn backend(e: impl std::fmt::Display) -> Self {
        Error::Backend(e.to_string())
    }

    pub fn is_conflict(&self) -> bool {
        matches!(self, Error::Conflict(_))
    }

    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Error::NotFound(_) | Error::KeyNotFound(_) | Error::ObjectNotFound { .. }
        )
    }
}

pub type Result<T> = std::result::Result<T, Error>;
