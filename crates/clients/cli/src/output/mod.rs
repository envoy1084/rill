//! Human and JSON output.
//!
//! Every command produces one JSON value (via [`rill_mcp::json`]) and a
//! human rendering of the same data. With `--json` the value is printed
//! as-is; otherwise the human rendering goes through [`Out`], which owns
//! the writer and the colour decision.

mod format;
pub mod pager;
mod table;
pub mod theme;
mod writer;

pub use format::{short, when, yes_no};
pub use table::Table;
pub use theme::{Role, Theme};
pub use writer::Out;
