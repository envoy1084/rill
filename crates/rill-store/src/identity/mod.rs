//! Instance-wide identity state: token signing keys, sessions and
//! revocations, and the change log every server follows to keep its
//! identity view current.

mod log;
mod logins;
mod providers;
mod revocations;
mod sessions;
mod store;
mod token_keys;
mod users;

pub use store::IdentityStore;
