//! Instance-wide identity state: token signing keys, sessions and
//! revocations, and the change log every server follows to keep its
//! identity view current.

mod agents;
mod budgets;
mod log;
mod logins;
mod providers;
mod revocations;
mod scim;
mod sessions;
mod store;
mod teams;
mod token_keys;
mod users;
mod workloads;

pub use store::IdentityStore;
