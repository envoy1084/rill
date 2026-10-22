//! Org secrets for actions: sealed storage, injection at dispatch and
//! masking in output.

mod inject;
mod redact;
mod store;

pub use redact::Redactor;
pub use store::{validate_secret_name, SecretInfo, SecretScope, MAX_SECRET_LEN, MIN_SECRET_LEN};
