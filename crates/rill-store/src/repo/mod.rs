//! Typed access to repositories over the two stores.

mod codec;
mod derived;
mod records;
mod relations;
mod store;
mod stores;
mod trunk;
mod versions;

pub use codec::{decode_record, decode_tagged, encode_record, encode_tagged};
pub use store::RepoStore;
pub use stores::{RepoConfig, RepoSettings, Stores, STORE_FORMAT};
