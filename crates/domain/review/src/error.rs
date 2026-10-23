use rill_types::{ChangeId, ChangeVersionHash, ReviewStatus};
use thiserror::Error;

/// Errors produced by the review state machine.
#[derive(Debug, Error)]
pub enum Error {
    /// No review state exists for the change.
    #[error("review not found for change {0}")]
    ReviewNotFound(ChangeId),

    /// The change has no stored versions, so it cannot be submitted.
    #[error("change {0} has no versions")]
    NoVersions(ChangeId),

    /// The version is not stored in the repository.
    #[error("change version not found: {0}")]
    VersionNotFound(ChangeVersionHash),

    /// The version belongs to a different change than the review.
    #[error("version {version} belongs to change {actual}, not {expected}")]
    WrongChange {
        version: ChangeVersionHash,
        expected: ChangeId,
        actual: ChangeId,
    },

    /// A review already exists for the change.
    #[error("review already exists for change {0}")]
    AlreadySubmitted(ChangeId),

    /// The operation is not valid in the change's current status.
    #[error("change {change_id} is {status:?}: {operation} not allowed")]
    InvalidStatus {
        change_id: ChangeId,
        status: ReviewStatus,
        operation: &'static str,
    },

    /// The label is not configured on the repository.
    #[error("unknown label {0:?}")]
    UnknownLabel(String),

    /// The vote value lies outside the label's range.
    #[error("vote {value} is outside the range {min}..={max} of label {label:?}")]
    VoteOutOfRange {
        label: String,
        value: i8,
        min: i8,
        max: i8,
    },

    /// Only humans may vote the maximum on this label.
    #[error("label {0:?} requires a human to vote its maximum value")]
    MaxRequiresHuman(String),

    /// No thread with that id exists on the change.
    #[error("thread {thread_id} not found on change {change_id}")]
    ThreadNotFound {
        change_id: ChangeId,
        thread_id: String,
    },

    /// The thread is already resolved.
    #[error("thread {0} is already resolved")]
    ThreadAlreadyResolved(String),

    /// The proposed stack does not form a first-parent chain.
    #[error("invalid stack: {0}")]
    InvalidStack(String),

    /// Optimistic concurrency retries were exhausted.
    #[error("too many concurrent updates to change {0}; giving up")]
    Contended(ChangeId),

    /// The change moved past the version an action was meant for.
    #[error("{version} is not the current version of change {change_id}")]
    StaleVersion {
        change_id: ChangeId,
        version: ChangeVersionHash,
    },

    /// A reviewer's result breaks its limits.
    #[error("reviewer result refused: {0}")]
    BadResult(String),

    /// The policy layer rejected the evaluation.
    #[error(transparent)]
    Policy(#[from] rill_policy::Error),

    /// A core algorithm failed.
    #[error(transparent)]
    Core(#[from] rill_core::Error),

    /// The store failed.
    #[error(transparent)]
    Store(#[from] rill_store::Error),

    /// A core type failed validation.
    #[error(transparent)]
    Types(#[from] rill_types::Error),

    /// A background task panicked or was cancelled.
    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
