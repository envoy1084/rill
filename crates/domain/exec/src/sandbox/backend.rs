//! The contract every execution backend implements.

use std::path::Path;

use async_trait::async_trait;

use super::RawOutcome;
use crate::action::Action;
use crate::error::Result;

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
