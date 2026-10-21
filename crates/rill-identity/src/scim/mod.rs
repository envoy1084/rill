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
mod service;

pub use error::ScimError;
pub use filter::Filter;
pub use service::{ListQuery, Scim};

/// The SCIM user schema.
pub const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
/// The SCIM group schema.
pub const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
/// List responses.
pub const LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
/// Patch requests.
pub const PATCH_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
/// Error responses.
pub const ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
