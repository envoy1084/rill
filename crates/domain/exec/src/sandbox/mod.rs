//! Execution backends.
//!
//! A [`Sandbox`] runs one [`Invocation`] in an already-staged directory
//! and reports what happened. The worker owns staging and output capture,
//! so a backend only needs to run a process.

mod backend;
#[cfg(target_os = "linux")]
mod confine;
mod invocation;
mod local;
mod outcome;
mod paths;

pub use backend::Sandbox;
pub use invocation::Invocation;
pub use local::{Isolation, LocalSandbox};
pub use outcome::{RawOutcome, TIMEOUT_EXIT_CODE};
pub use paths::local_path;
