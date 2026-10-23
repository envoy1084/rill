//! Human and JSON output.
//!
//! Every command produces one JSON value (via [`rill_mcp::json`]) and a
//! human rendering of the same data. With `--json` the value is printed
//! as-is; otherwise the human rendering goes through [`Out`], which owns
//! the writer and the colour decision.

pub mod diff;
pub mod editor;
pub mod format;
pub mod graph;
pub mod pager;
pub mod patch;
pub mod progress;
pub mod stat;
mod table;
pub mod theme;
mod writer;

pub use format::{ago, ago_from, short, strip_ansi, visible_width, when, yes_no};
pub use patch::{FileStatus, PatchFile, PatchMode};
pub use table::Table;
pub use theme::{Role, Theme};
pub use writer::Out;
