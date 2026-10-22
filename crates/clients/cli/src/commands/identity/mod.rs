//! `rill admin idp|user|key|revoke`: identity providers, users, token keys
//! and revocation.

pub mod agents;
pub mod keys;
pub mod providers;
mod revoke;
pub mod scim;
pub mod users;

pub use agents::{AgentCmd, KillArgs, TrustCmd};
pub use keys::KeyCmd;
pub use providers::IdpCmd;
pub use revoke::{revoke, RevokeCmd, RevokeSessionArgs, RevokeTokenArgs};
pub use scim::ScimCmd;
pub use users::UserCmd;
