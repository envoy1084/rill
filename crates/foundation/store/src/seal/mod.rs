//! Encryption of stored objects at rest.

mod org;
mod sealer;

pub use org::{DataKey, DataKeySource, OrgSealer, KEY_REFRESH};
pub use sealer::{org_of, ObjectAt, Plaintext, Sealer, DEFAULT_NAMESPACE};
