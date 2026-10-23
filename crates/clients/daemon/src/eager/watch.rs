//! The watcher of an eager mount. Events only name paths; a task
//! reconciles them shortly after. `settle` makes a caller wait until
//! every earlier edit is reconciled, by writing a cookie file and waiting
//! for its event (events arrive in order), the technique Watchman uses.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use parking_lot::Mutex;
use rill_types::RepoPathBuf;
use tokio::sync::{oneshot, Notify, OwnedMutexGuard};

use super::disk::to_repo_path;
use super::ignore::{IgnoreRules, IGNORE_FILE, TEMP_PREFIX};
use super::reconcile::{reconcile, walk_files};
use crate::error::Result;
use crate::mount::Mount;

/// How long edits coalesce before they are reconciled.
const COALESCE: Duration = Duration::from_millis(20);
/// How long `settle` waits for its cookie before rescanning instead.
const COOKIE_TIMEOUT: Duration = Duration::from_secs(5);
const COOKIE_PREFIX: &str = ".rill-tmp-cookie-";

#[derive(Default)]
struct Queue {
    paths: BTreeSet<RepoPathBuf>,
    rescan: bool,
}

/// An eager mount's watcher and reconcile queue.
pub struct Watch {
    root: PathBuf,
    rules: IgnoreRules,
    queue: Mutex<Queue>,
    wake: Notify,
    cookies: Mutex<HashMap<String, oneshot::Sender<()>>>,
    next_cookie: AtomicU64,
    /// Held while reconciling, and by updates writing files themselves.
    lock: Arc<tokio::sync::Mutex<()>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Watch {
    /// Watch `mount`'s directory and reconcile in the background until the
    /// mount is cancelled. Starts with a full scan, which finds edits made
    /// while the daemon was not running.
    pub(crate) fn start(mount: &Arc<Mount>) -> Result<Arc<Watch>> {
        let root = mount.record().path;
        let watch = Arc::new(Watch {
            rules: IgnoreRules::new(&root),
            root: root.clone(),
            queue: Mutex::new(Queue {
                rescan: true,
                ..Queue::default()
            }),
            wake: Notify::new(),
            cookies: Mutex::new(HashMap::new()),
            next_cookie: AtomicU64::new(0),
            lock: Arc::new(tokio::sync::Mutex::new(())),
            watcher: Mutex::new(None),
        });
        let weak = Arc::downgrade(&watch);
        let mut watcher = notify::recommended_watcher(move |event| {
            if let Some(watch) = weak.upgrade() {
                watch.on_event(event);
            }
        })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        *watch.watcher.lock() = Some(watcher);
        let task = watch.clone();
        let mount = mount.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = mount.cancel.cancelled() => break,
                    _ = task.wake.notified() => {}
                }
                tokio::time::sleep(COALESCE).await;
                if let Err(e) = task.drain(&mount).await {
                    tracing::warn!(mount = %mount.id(), error = %e, "reconcile failed");
                }
            }
            task.watcher.lock().take();
        });
        watch.wake.notify_one();
        Ok(watch)
    }

    fn on_event(&self, event: notify::Result<notify::Event>) {
        let event = match event {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(root = %self.root.display(), error = %e, "watch error; rescanning");
                self.queue.lock().rescan = true;
                self.wake.notify_one();
                return;
            }
        };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        let mut queue = self.queue.lock();
        if event.need_rescan() {
            queue.rescan = true;
        }
        for path in &event.paths {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if let Some(cookie) = name.strip_prefix(COOKIE_PREFIX) {
                if let Some(tx) = self.cookies.lock().remove(cookie) {
                    let _ = tx.send(());
                }
                continue;
            }
            if name.starts_with(TEMP_PREFIX) {
                continue;
            }
            if name == IGNORE_FILE {
                if let Some(dir) = path.parent() {
                    self.rules.invalidate(dir);
                }
            }
            match path.strip_prefix(&self.root).map(to_repo_path) {
                Ok(Ok(p)) if !p.is_root() => {
                    queue.paths.insert(p);
                }
                Ok(Ok(_)) => queue.rescan = true,
                _ => {}
            }
        }
        drop(queue);
        self.wake.notify_one();
    }

    /// Reconcile everything queued.
    async fn drain(&self, mount: &Mount) -> Result<()> {
        let _held = self.lock.lock().await;
        let queue = std::mem::take(&mut *self.queue.lock());
        let paths = if queue.rescan {
            self.everything(mount).await?
        } else {
            queue.paths
        };
        if paths.is_empty() {
            return Ok(());
        }
        reconcile(mount, &self.rules, paths).await
    }

    /// Every path a full scan visits: on disk, in the stat cache, in the
    /// overlay.
    async fn everything(&self, mount: &Mount) -> Result<BTreeSet<RepoPathBuf>> {
        let root = self.root.clone();
        let mut paths: BTreeSet<RepoPathBuf> =
            tokio::task::spawn_blocking(move || walk_files(&root, &root))
                .await??
                .into_iter()
                .collect();
        let id = mount.id();
        let stats = mount.shared.db(move |db| db.files(&id)).await?;
        paths.extend(stats.into_iter().map(|(p, _)| p));
        paths.extend(mount.entries().into_iter().map(|(p, _)| p));
        Ok(paths)
    }

    /// Return once every edit made before the call is in the overlay.
    pub(crate) async fn settle(self: Arc<Self>, mount: &Mount) -> Result<()> {
        let n = self.next_cookie.fetch_add(1, Ordering::Relaxed).to_string();
        let (tx, rx) = oneshot::channel();
        self.cookies.lock().insert(n.clone(), tx);
        let cookie = cookie_path(&self.root, &n);
        let written = cookie.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(dir) = written.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&written, b"")
        })
        .await??;
        if tokio::time::timeout(COOKIE_TIMEOUT, rx).await.is_err() {
            tracing::warn!(root = %self.root.display(), "watcher is slow; rescanning");
            self.cookies.lock().remove(&n);
            self.queue.lock().rescan = true;
        }
        let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(cookie)).await?;
        self.drain(mount).await
    }

    /// Hold off reconciling (while an update writes files itself).
    pub(crate) async fn quiesce(&self) -> OwnedMutexGuard<()> {
        self.lock.clone().lock_owned().await
    }
}

fn cookie_path(root: &Path, n: &str) -> PathBuf {
    root.join(".rill").join(format!("{COOKIE_PREFIX}{n}"))
}
