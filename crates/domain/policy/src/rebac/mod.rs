//! Relationship-based access control.
//!
//! Access is a set of `(object, relation, subject)` tuples stored in the
//! repository's metadata (`RepoStore::add_relation`). Objects are the
//! repository (`repo`) or directories (`dir:/src/billing`); subjects are
//! principals, teams, principal kinds or everyone. [`Authorizer::check`]
//! applies the relation order (`owner ⊃ writer ⊃ reviewer ⊃ reader`) and
//! directory inheritance over a versioned snapshot of the tuples.

mod authorizer;
mod cache;
mod model;
mod path_filter;
mod snapshot;
#[cfg(test)]
mod tests;

pub use authorizer::Authorizer;
pub use cache::{SnapshotCache, DEFAULT_SNAPSHOT_TTL};
pub use model::{dir_object, dir_of_object, inheritance_chain, Relation, Subject, REPO_OBJECT};
pub use path_filter::PathFilter;
pub use snapshot::{Snapshot, Tuple};
