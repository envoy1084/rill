//! The SCIM 2.0 endpoint each organization's provisioning system calls:
//! `/scim/v2/<org>/Users`, `/Groups` and the discovery resources. Callers
//! present a SCIM credential of that org (`scim_<id>.<secret>`).

mod credential;
mod discovery;
mod resources;
mod response;
mod router;
mod service;

pub use response::{scim_error, scim_json};
pub use router::{router, MAX_SCIM_BODY};
pub use service::scim_for;
