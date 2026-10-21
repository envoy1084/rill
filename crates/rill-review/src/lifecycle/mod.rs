//! The review lifecycle: submit, new versions, and status transitions.

mod interdiff;
mod retry;
mod status;
mod submit;
mod versions;

pub use interdiff::VersionRelation;
pub(crate) use retry::service_actor;
pub use retry::MAX_WRITE_ATTEMPTS;
pub use versions::PreparedVersion;
