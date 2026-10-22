//! Post-land verification: follow trunk, verify, bisect, revert.

mod record;
mod reverts;
mod worker;

pub use record::{
    load_cursor, recent_records, record_at, record_of, VerificationRecord, VerificationStatus,
    POSTLAND_CURSOR_MISC, POSTLAND_INDEX_MISC, POSTLAND_KIND, POSTLAND_RING,
};
pub use reverts::{schedule_auto_revert, RevertHandler, REVERT_JOB, REVERT_QUEUE};
pub use worker::{PostLandVerifier, Sweep, BISECT_WAYS, DEFAULT_BATCH_SIZE, POSTLAND_LEASE};
