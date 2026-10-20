//! Extension point for VM-isolated execution.
//!
//! Multi-tenant servers need stronger isolation than a host process: a
//! microVM (Firecracker) or a user-space kernel (gVisor) that gets a
//! read-only image of the input tree, an explicit network policy and a
//! resource budget. The [`Sandbox`] contract is the same; only `run`
//! differs. Such a backend would:
//!
//! 1. build or reuse a root filesystem image for `action.platform`;
//! 2. expose `dir` to the guest (virtio-fs or a packed block device);
//! 3. boot, run the command with the action's environment and timeout,
//!    with networking enabled only when `network` is set;
//! 4. copy `output_paths` back into `dir` before returning, since the
//!    runner collects outputs from the host-side directory.
//!
//! The runner, cache and verification plan need no changes for that.

use std::path::Path;

use async_trait::async_trait;

use super::{RawOutcome, Sandbox};
use crate::action::Action;
use crate::error::{Error, Result};

/// Configuration a Firecracker-backed sandbox would need.
#[derive(Debug, Clone)]
pub struct FirecrackerSandbox {
    /// Whether guests get a network interface.
    pub network: bool,
    /// Path of the kernel image to boot.
    pub kernel: std::path::PathBuf,
    /// Path of the root filesystem image.
    pub rootfs: std::path::PathBuf,
    /// Number of virtual CPUs per guest.
    pub vcpus: u32,
    /// Guest memory in MiB.
    pub memory_mib: u32,
}

#[async_trait]
impl Sandbox for FirecrackerSandbox {
    async fn run(&self, _dir: &Path, _action: &Action) -> Result<RawOutcome> {
        Err(Error::NotSupported {
            backend: "firecracker",
            reason: "VM-backed execution is not implemented in this build; use LocalSandbox"
                .to_string(),
        })
    }

    fn worker_name(&self) -> String {
        "firecracker".to_string()
    }
}
