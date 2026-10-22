//! `proptest` strategies for Rill's core types and a controllable clock,
//! shared by property tests, the simulator and fuzz harnesses (feature
//! `testing`).

mod changes;
pub(crate) mod clock;
mod content;
mod merge;
mod paths;
mod text;

pub use changes::{change_version, file_change, file_state};
pub use clock::{set_clock, set_id_seed};
pub use content::{content_ref, object_hash};
pub use merge::merge_of;
pub use paths::{component, invalid_component, path};
pub use text::{edited, text};
