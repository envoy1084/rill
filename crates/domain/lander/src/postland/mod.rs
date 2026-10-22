//! Post-land verification: follow trunk, verify, bisect, revert.

mod batch;
mod bisect;
mod positions;
mod record;
mod reverts;
mod worker;

pub use batch::BatchSize;
pub use bisect::BISECT_WAYS;
pub use record::{
    load_cursor, load_marks, recent_records, record_at, record_of, Marks, VerificationRecord,
    VerificationStatus, POSTLAND_CURSOR_MISC, POSTLAND_INDEX_MISC, POSTLAND_KIND, POSTLAND_RING,
};
pub use reverts::{schedule_auto_revert, RevertHandler, REVERT_JOB, REVERT_QUEUE};
pub use worker::{PostLandVerifier, Sweep, DEFAULT_BATCH_SIZE, DEFAULT_FULL_EVERY, POSTLAND_LEASE};
