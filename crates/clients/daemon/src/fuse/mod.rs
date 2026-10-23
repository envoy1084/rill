//! The Linux front end of the view: FUSE through `fuser`, mounted with
//! `fusermount3`, with reads of cached files handed to the kernel where it
//! allows passthrough.

mod fs;

use std::sync::Arc;

use crate::error::Result;
use crate::mount::Mount;
use crate::vfs::{Session, View};

/// Mount `view` at `mount`'s directory.
pub(crate) fn start(mount: &Arc<Mount>, view: Arc<View>) -> Result<Session> {
    use fuser::MountOption;

    let path = mount.record().path;
    clear_stale_mount(&path);
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

/// A mount left by a daemon that died answers `ENOTCONN`; detach it so
/// the directory can be mounted again.
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
