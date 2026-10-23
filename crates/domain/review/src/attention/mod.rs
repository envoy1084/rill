//! Human attention: what needs a person (the inbox), sampled audits of
//! agent-reviewed landings, batch review of look-alike changes, and daily
//! digests per intent.

mod audits;
mod digest;
mod inbox;
mod similar;

pub use audits::{sampled, Audit, AuditStatus, DEFAULT_SAMPLE_PERMILLE};
pub use digest::{escaped_key, IntentDigest};
pub use inbox::{InboxItem, InboxKind, Reader};
pub use similar::SimilarGroup;
