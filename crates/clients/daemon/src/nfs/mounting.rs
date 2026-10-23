//! Mounting a loopback NFS export with macOS's own client, and taking it
//! down. A user may mount onto a directory they own; no privileges are
//! needed.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// `mount_nfs` options: NFSv3 over TCP to the daemon's port, no lock
/// daemon, out of the Finder sidebar, short attribute caching so updates
/// show quickly, and soft so a dead daemon fails calls instead of hanging
/// them (after about a minute).
fn options(port: u16) -> String {
    format!(
        "vers=3,tcp,port={port},mountport={port},nolocks,locallocks,nobrowse,soft,\
         timeo=300,retrans=2,rsize=131072,wsize=131072,actimeo=1"
    )
}

/// Mount the export on 127.0.0.1:`port` at `path`.
pub async fn mount(port: u16, path: &Path) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let out = Command::new("/sbin/mount_nfs")
            .arg("-o")
            .arg(options(port))
            .arg("127.0.0.1:/")
            .arg(&path)
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(Error::Unsupported(format!(
                "mount_nfs at {} failed: {}",
                path.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    })
    .await?
}

/// Unmount `path`, forcibly when it is busy.
pub fn unmount(path: &Path) {
    let ok = Command::new("/sbin/umount")
        .arg(path)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        let status = Command::new("/usr/sbin/diskutil")
            .args(["unmount", "force"])
            .arg(path)
            .status();
        tracing::info!(path = %path.display(), ?status, "forced unmount");
    }
}

/// Unmount an NFS mount left at `path` by a daemon that died.
pub fn clear_stale(path: &Path) {
    if is_nfs_mount(path) {
        tracing::info!(path = %path.display(), "detaching a stale mount");
        unmount(path);
    }
}

/// Whether `path` is itself the root of an NFS mount.
fn is_nfs_mount(path: &Path) -> bool {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;

    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a NUL-terminated path and `st` a writable statfs.
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return false;
    }
    // SAFETY: statfs fills both fields with NUL-terminated strings.
    let (on, kind) = unsafe {
        (
            CStr::from_ptr(st.f_mntonname.as_ptr()),
            CStr::from_ptr(st.f_fstypename.as_ptr()),
        )
    };
    kind.to_bytes() == b"nfs" && on.to_bytes() == path.as_os_str().as_bytes()
}
