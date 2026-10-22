//! The identity service: token keys, revocation and sessions.

pub mod agents;
mod keys;
mod kill;
mod providers;
mod revoke;
pub mod scim_credentials;
mod service;
mod users;
mod workloads;

pub use service::IdentitySvc;
