//! Execution backends.
//!
//! A [`Sandbox`] runs one [`Action`] in an already-materialized directory
//! and reports what happened. The runner owns materialization, caching and
//! output capture, so a backend only needs to run a process.

#[cfg(target_os = "linux")]
mod confine;
mod local;

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use crate::action::Action;
use crate::error::Result;

pub use local::{Isolation, LocalSandbox};

/// Raw outcome of running a process, before anything is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutcome {
    /// Process exit code, or [`crate::action::TIMEOUT_EXIT_CODE`] on
    /// timeout and `-1` when killed by a signal.
    pub exit_code: i32,
    pub stdout: Bytes,
    pub stderr: Bytes,
    pub duration: Duration,
    /// True when the process was killed because the timeout expired.
    pub timed_out: bool,
}

/// Something that can run an action's command in a prepared directory.
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Run `action.command` with `dir` as the materialized input root.
    async fn run(&self, dir: &Path, action: &Action) -> Result<RawOutcome>;

    /// Identity recorded as the worker on results this sandbox produces.
    fn worker_name(&self) -> String {
        "local".to_string()
    }
}
