use rill_types::{ChangeVersionHash, TrunkSeq};
use thiserror::Error;

/// Errors produced by the landing pipeline.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    #[error(transparent)]
    Core(#[from] rill_core::Error),

    #[error(transparent)]
    Jobs(#[from] rill_jobs::Error),

    #[error(transparent)]
    Types(#[from] rill_types::Error),

    #[error("change version not found: {0}")]
    ChangeNotFound(ChangeVersionHash),

    #[error("nothing landed at {0}")]
    NotLanded(TrunkSeq),

    #[error("land ticket {0} is unknown")]
    UnknownTicket(u64),

    #[error("the lander is shutting down")]
    Shutdown,

    #[error("review hook failed: {0}")]
    Hook(String),

    #[error("another lander took over this repository")]
    Fenced,

    #[error("verifier failed: {0}")]
    Verifier(String),

    #[error("revert of {seq} did not land: {reason}")]
    RevertFailed { seq: TrunkSeq, reason: String },

    #[error("codec error: {0}")]
    Codec(String),

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
