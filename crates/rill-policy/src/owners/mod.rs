//! Code owners: who may approve changes to which directories, from the
//! repository's `.rill/owners.toml`.

mod file;

pub use file::{OwnerRule, OwnersFile, OWNERS_PATH};
