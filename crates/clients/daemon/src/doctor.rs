//! Host checks behind `rill doctor`: things on this machine that make
//! checkouts slow or broken, each with a fix.

use std::path::Path;

use crate::daemon::Daemon;
use crate::mount::MountMode;

/// Below this share of free space on the cache volume, eviction churns.
const LOW_FREE_PERCENT: u64 = 10;

/// Something worth fixing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// What it concerns: `cache`, `daemon`, or a checkout path.
    pub subject: String,
    pub message: String,
    pub fix: String,
}

fn warn(subject: impl Into<String>, message: impl Into<String>, fix: impl Into<String>) -> Warning {
    Warning {
        subject: subject.into(),
        message: message.into(),
        fix: fix.into(),
    }
}

/// Every check, run now. Blocking (it stats paths and may run `mdutil`);
/// call it off the async runtime.
pub fn check(daemon: &Daemon) -> Vec<Warning> {
    let mut out = Vec::new();
    let config = daemon.config();
    if let Some((size, free)) = crate::config::disk_space(&config.dir.cache()) {
        if size > 0 && free * 100 / size < LOW_FREE_PERCENT {
            out.push(warn(
                "cache",
                format!(
                    "{}% free on the cache's volume; eviction is active",
                    free * 100 / size
                ),
                "free disk space, or lower the cache budget",
            ));
        }
        let used = daemon.cache_stats().bytes;
        if config.cache_budget > free + used {
            out.push(warn(
                "cache",
                format!(
                    "the cache budget ({} GiB) exceeds the space it could use ({} GiB)",
                    config.cache_budget >> 30,
                    (free + used) >> 30
                ),
                "lower the cache budget",
            ));
        }
    }
    lazy_support(&mut out);
    let reads = daemon.read_stats();
    if reads.regional_fallbacks > 0 && reads.regional().reads == 0 {
        out.push(warn(
            "daemon",
            format!(
                "a regional cache is configured but served none of {} reads",
                reads.regional_fallbacks
            ),
            "check RILL_CACHE_URL and that the cache can reach the origin",
        ));
    }
    for mount in daemon.mounts() {
        let record = mount.record();
        let at = record.path.display().to_string();
        if let Some(e) = mount.sync_error.lock().clone() {
            let fix = if e.to_ascii_lowercase().contains("unauthenticated") {
                "sign in again: `rill login`"
            } else {
                "edits stay local and sync when the server answers; see `rill daemon status`"
            };
            out.push(warn(&at, format!("sync failing: {e}"), fix));
        }
        if record.mode != MountMode::Eager {
            if let Err(e) = std::fs::metadata(&record.path) {
                out.push(warn(
                    &at,
                    format!("the view does not answer: {e}"),
                    "restart the daemon: `rill daemon stop`, then any rill command",
                ));
            } else if indexed(&record.path) {
                out.push(warn(
                    &at,
                    "Spotlight indexes this checkout, which fetches every file",
                    format!("mdutil -i off {at}"),
                ));
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn lazy_support(out: &mut Vec<Warning>) {
    if !Path::new("/dev/fuse").exists() {
        out.push(warn(
            "daemon",
            "/dev/fuse is missing, so lazy checkouts are unavailable",
            "load the fuse module (`modprobe fuse`) or use eager checkouts",
        ));
    } else if std::process::Command::new("fusermount3")
        .arg("--version")
        .output()
        .is_err()
    {
        out.push(warn(
            "daemon",
            "fusermount3 is missing, so lazy checkouts are unavailable",
            "install fuse3 (`apt install fuse3`)",
        ));
    }
}

#[cfg(target_os = "macos")]
fn lazy_support(out: &mut Vec<Warning>) {
    if !Path::new("/sbin/mount_nfs").exists() {
        out.push(warn(
            "daemon",
            "mount_nfs is missing, so lazy checkouts are unavailable",
            "use eager checkouts",
        ));
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn lazy_support(out: &mut Vec<Warning>) {
    out.push(warn(
        "daemon",
        "lazy checkouts are not available on this platform",
        "use eager checkouts",
    ));
}

/// Whether Spotlight indexes the volume at `path`.
#[cfg(target_os = "macos")]
fn indexed(path: &Path) -> bool {
    std::process::Command::new("/usr/bin/mdutil")
        .arg("-s")
        .arg(path)
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("Indexing enabled"))
}

#[cfg(not(target_os = "macos"))]
fn indexed(_path: &Path) -> bool {
    false
}
