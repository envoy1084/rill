//! Lazy and snapshot mounts: a FUSE view of the overlay over the base.
//! `readdir` and `stat` read trees only; contents are fetched into the
//! object cache on first open, and reads of cached files go straight to
//! the kernel where it supports passthrough. Writes land in the overlay
//! when a file is flushed. Linux only; the module builds on macOS so it
//! lints there, but never mounts.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

mod attrs;
mod fs;
mod handles;
mod inodes;
mod node;
mod ops;
mod view;

use std::sync::Arc;

use bytes::Bytes;

use crate::error::Result;
use crate::mount::Mount;

/// Something that keeps a view mounted until it is dropped.
pub type Session = Box<dyn Send>;

/// Mount `mount`'s view at its directory, showing `marker` as the
/// checkout marker.
#[cfg(target_os = "linux")]
pub(crate) fn start(mount: &Arc<Mount>, marker: Bytes) -> Result<Session> {
    use fuser::MountOption;

    let path = mount.record().path;
    clear_stale_mount(&path);
    let view = Arc::new(view::View::new(
        mount.clone(),
        tokio::runtime::Handle::current(),
        marker,
    ));
    let mut config = fuser::Config::default();
    config.mount_options = vec![
        MountOption::FSName("rill".into()),
        MountOption::Subtype("rill".into()),
        MountOption::DefaultPermissions,
        MountOption::NoAtime,
    ];
    config.n_threads = Some(8);
    config.clone_fd = true;
    let session = fuser::spawn_mount(fs::Fs(view), &path, &config)?;
    Ok(Box::new(session))
}

/// Lazy views need Linux.
#[cfg(not(target_os = "linux"))]
pub(crate) fn start(_mount: &Arc<Mount>, _marker: Bytes) -> Result<Session> {
    Err(crate::error::Error::Unsupported(
        "lazy and snapshot mounts need Linux; use an eager checkout".into(),
    ))
}

/// A mount left by a daemon that died answers `ENOTCONN`; detach it so
/// the directory can be mounted again.
#[cfg(target_os = "linux")]
fn clear_stale_mount(path: &std::path::Path) {
    if let Err(e) = std::fs::metadata(path) {
        if e.raw_os_error() == Some(libc::ENOTCONN) {
            let status = std::process::Command::new("fusermount3")
                .arg("-u")
                .arg("-z")
                .arg(path)
                .status();
            tracing::info!(path = %path.display(), ?status, "detached a stale mount");
        }
    }
}
