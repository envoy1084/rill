//! Ancestry queries over the change graph.

mod commit_graph;
mod walk;

pub use commit_graph::CommitGraph;
pub use walk::LogEntry;
