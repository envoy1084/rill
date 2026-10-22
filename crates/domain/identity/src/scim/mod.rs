//! SCIM 2.0 (RFC 7643, 7644) provisioning: users and groups as a
//! provisioning system (Okta, Entra ID) sees them, mapped onto users and
//! teams. Groups become the teams relation checks use, and deprovisioning
//! a user suspends them, ends their sessions and suspends the agents they
//! sponsor.

mod error;
mod filter;
mod groups;
mod patch;
mod resources;
mod schemas;
mod service;

pub use error::ScimError;
pub use filter::Filter;
pub use patch::patch_user;
pub use resources::UserFields;
pub use schemas::{ERROR_SCHEMA, GROUP_SCHEMA, LIST_SCHEMA, PATCH_SCHEMA, USER_SCHEMA};
pub use service::{ListQuery, Scim};
