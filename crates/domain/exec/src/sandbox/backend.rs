//! The contract every execution backend implements.

use std::path::Path;

use async_trait::async_trait;

use super::{Invocation, RawOutcome};
use crate::error::Result;

/// Something that can run a command in a prepared directory.
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Run `invocation` with `dir` as the staged input root.
    async fn run(&self, dir: &Path, invocation: &Invocation) -> Result<RawOutcome>;
}
