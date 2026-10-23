//! Where the daemon keeps its files and how it behaves.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rill_sdk::ClientConfig;

use crate::error::{Error, Result};

/// Names the daemon directory instead of the per-user default.
pub const DIR_ENV: &str = "RILL_DAEMON_DIR";

/// The largest default cache budget.
const MAX_DEFAULT_BUDGET: u64 = 200 * 1024 * 1024 * 1024;

/// The daemon's directory: socket, state database and object cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonDir(PathBuf);

impl DaemonDir {
    /// The directory at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        DaemonDir(root.into())
    }

    /// `$RILL_DAEMON_DIR`, else the user's local data directory.
    pub fn from_env() -> Result<Self> {
        if let Some(dir) = std::env::var_os(DIR_ENV).filter(|d| !d.is_empty()) {
            return Ok(DaemonDir::new(dir));
        }
        dirs::data_local_dir()
            .map(|d| DaemonDir::new(d.join("rill").join("daemon")))
            .ok_or_else(|| {
                Error::Unavailable("no local data directory; set RILL_DAEMON_DIR".into())
            })
    }

    /// The directory itself.
    pub fn root(&self) -> &Path {
        &self.0
    }

    /// The local socket clients connect to.
    pub fn socket(&self) -> PathBuf {
        self.0.join("rilld.sock")
    }

    /// The state database.
    pub fn state(&self) -> PathBuf {
        self.0.join("state.redb")
    }

    /// The object cache.
    pub fn cache(&self) -> PathBuf {
        self.0.join("cache")
    }

    /// Scratch space for lazy mounts' written files.
    pub fn scratch(&self) -> PathBuf {
        self.0.join("scratch")
    }

    /// The log of a daemon started in the background.
    pub fn log(&self) -> PathBuf {
        self.0.join("rilld.log")
    }
}

/// Builds the client configuration for an endpoint, given the token the
/// last caller passed for it, if any.
pub type ConfigSource = Arc<dyn Fn(&str, Option<String>) -> ClientConfig + Send + Sync>;

/// How the daemon runs.
#[derive(Clone)]
pub struct DaemonConfig {
    /// Its directory.
    pub dir: DaemonDir,
    /// Bytes the object cache may hold (pinned objects aside).
    pub cache_budget: u64,
    /// How long local edits wait before they are pushed.
    pub sync_delay: Duration,
    /// Client configuration per endpoint.
    pub clients: ConfigSource,
}

impl std::fmt::Debug for DaemonConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonConfig")
            .field("dir", &self.dir)
            .field("cache_budget", &self.cache_budget)
            .field("sync_delay", &self.sync_delay)
            .finish_non_exhaustive()
    }
}

impl DaemonConfig {
    /// Defaults for `dir`: a cache of 20% of its disk up to 200 GB, a
    /// two-second sync delay, and a token from the caller or `RILL_TOKEN`.
    pub fn new(dir: DaemonDir) -> Self {
        let cache_budget = default_budget(dir.root());
        DaemonConfig {
            dir,
            cache_budget,
            sync_delay: Duration::from_secs(2),
            clients: Arc::new(|endpoint, token| {
                let mut config = ClientConfig::new(endpoint);
                config.token = token.or_else(|| {
                    std::env::var(rill_sdk::config::TOKEN_ENV)
                        .ok()
                        .filter(|t| !t.is_empty())
                });
                config.user_agent = format!("rilld/{}", env!("CARGO_PKG_VERSION"));
                config
            }),
        }
    }
}

fn default_budget(dir: &Path) -> u64 {
    let probe = dir
        .ancestors()
        .find(|d| d.exists())
        .unwrap_or(Path::new("/"));
    disk_size(probe).map_or(MAX_DEFAULT_BUDGET / 10, |size| {
        (size / 5).min(MAX_DEFAULT_BUDGET)
    })
}

#[cfg(unix)]
fn disk_size(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` a writable
    // statvfs the call fills in.
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    (rc == 0).then(|| st.f_blocks as u64 * st.f_frsize as u64)
}

#[cfg(not(unix))]
fn disk_size(_path: &Path) -> Option<u64> {
    None
}
