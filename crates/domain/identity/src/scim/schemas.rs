//! SCIM schema URNs carried in every resource and message.

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
