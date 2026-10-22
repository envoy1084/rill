//! Simulated agents and reviewers, and the actions they take.

mod action;
mod agent;
mod authoring;
mod ctx;
mod editing;
mod reviewing;

pub use action::{Action, Outcome};
pub use agent::{Agent, Owned};
pub use ctx::Ctx;
pub use editing::{FILES, FILE_LINES, HOT_FILE};
