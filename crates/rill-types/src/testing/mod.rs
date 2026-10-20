//! `proptest` strategies for Rill's core types, shared by property tests,
//! the simulator and fuzz harnesses (feature `testing`).

mod changes;
mod content;
mod merge;
mod paths;
mod text;

pub use changes::{change_version, file_change, file_state};
pub use content::{content_ref, object_hash};
pub use merge::merge_of;
pub use paths::{component, invalid_component, path};
pub use text::{edited, text};
