//! Execution backends.
//!
//! A [`Sandbox`] runs one [`Action`] in an already-materialized directory
//! and reports what happened. The runner owns materialization, caching and
//! output capture, so a backend only needs to run a process.

mod backend;
#[cfg(target_os = "linux")]
mod confine;
mod local;
mod outcome;

pub use backend::Sandbox;
pub use local::{Isolation, LocalSandbox};
pub use outcome::RawOutcome;
