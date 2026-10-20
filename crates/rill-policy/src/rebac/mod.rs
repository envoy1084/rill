//! Relationship-based access control.
//!
//! Access is a set of `(object, relation, subject)` tuples stored in the
//! repository's metadata (`RepoStore::add_relation`). Objects are the
//! repository (`repo`) or directories (`dir:/src/billing`); subjects are
//! principals, teams, principal kinds or everyone. [`Authorizer::check`]
//! applies userset rewrites (`owner ⊇ writer ⊇ reader`) and directory
//! inheritance over a short-lived snapshot of the tuples.

mod authorizer;
mod model;
mod snapshot;
#[cfg(test)]
mod tests;

pub use authorizer::{Authorizer, PathFilter, DEFAULT_SNAPSHOT_TTL};
pub use model::{dir_object, inheritance_chain, Relation, Subject, REPO_OBJECT};
pub use snapshot::{Snapshot, SnapshotCache, Tuple};
