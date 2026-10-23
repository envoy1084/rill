//! `rilld`, the client daemon every local tool on a host shares. It owns
//! the object cache and the working copies (mounts), records local edits
//! in a durable overlay, syncs them to each mount's server workspace, and
//! answers `status` from the overlay without walking the tree.

pub mod cache;
pub mod client;
mod clients;
pub mod config;
mod daemon;
pub mod doctor;
pub mod eager;
pub mod error;
#[cfg(target_os = "linux")]
mod fuse;
pub mod mount;
#[cfg(target_os = "macos")]
mod nfs;
pub mod overlay;
pub mod proto;
pub mod server;
mod shared;
pub mod state;
mod sync;
#[cfg(unix)]
mod vfs;
pub mod view;

pub use client::{connect_or_start, DaemonClient};
pub use config::{DaemonConfig, DaemonDir};
pub use daemon::{Daemon, Marker, MountSpec};
pub use error::{Error, Result};
pub use mount::{Mount, MountMode};
pub use overlay::{ChangeKind, LocalChange};
pub use server::serve;
