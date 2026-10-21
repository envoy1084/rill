//! The audit log: what happened, who did it, and a hash chain per org that
//! makes any later edit or deletion detectable.

mod entry;
mod root;

pub use entry::{AuditEntry, AuditHead, AuditOutcome, AuditRecord, INSTANCE_AUDIT};
pub use root::AuditRoot;
