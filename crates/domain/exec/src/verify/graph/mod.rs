//! Build graphs: which targets of a Bazel or Buck2 repository a change
//! affects, from target hashes a query action computes per tree.

mod affected;
mod hashes;
mod query;
mod spec;

pub use affected::affected_targets;
pub use hashes::{parse_target_hashes, TargetHashes};
pub(crate) use spec::GraphFile;
pub use spec::{GraphSpec, DEFAULT_GRAPH_TIMEOUT, DEFAULT_OUTPUT, GRAPH_STEP};
