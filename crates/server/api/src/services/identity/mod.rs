//! `rill.v1.IdentityService`: the caller, tokens and their keys, and each
//! org's identity providers, users, agents, workload issuers and SCIM
//! credentials.
//!
//! Capability scopes are `repo:<repo>` or `repo:<repo>/<dir>[/*]`.

pub mod agents;
mod keys;
mod kill;
mod providers;
mod revoke;
pub mod scim_credentials;
mod scopes;
mod service;
mod tokens;
mod users;
mod workloads;

pub use service::IdentitySvc;
