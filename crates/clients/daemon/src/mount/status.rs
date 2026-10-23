//! `status`: the overlay compared with the base, O(local edits).

use rill_types::RepoPathBuf;

use super::mount::Mount;
use crate::error::Result;
use crate::overlay::{classify, LocalChange};

impl Mount {
    /// Paths that differ from the base, under any of `prefixes` (every
    /// path when empty), in path order.
    pub async fn status(&self, prefixes: &[RepoPathBuf]) -> Result<Vec<LocalChange>> {
        self.settle().await?;
        self.changes(prefixes).await
    }

    /// [`Mount::status`] without waiting for the watcher.
    pub(crate) async fn changes(&self, prefixes: &[RepoPathBuf]) -> Result<Vec<LocalChange>> {
        let base = self.base();
        let mut out = Vec::new();
        for (path, entry) in self.entries() {
            if !prefixes.is_empty() && !prefixes.iter().any(|p| path.starts_with(p)) {
                continue;
            }
            let file = self.base_file_in(&base, &path).await?;
            if let Some(kind) = classify(&entry, file.as_ref()) {
                out.push(LocalChange { path, kind });
            }
        }
        Ok(out)
    }

    /// Wait until every edit made to the working copy so far is in the
    /// overlay. Eager mounts learn of edits from a watcher, which lags.
    pub async fn settle(&self) -> Result<()> {
        if let Some(watch) = self.watch.get() {
            return watch.clone().settle(self).await;
        }
        match self.settler.get() {
            Some(settle) => settle().await,
            None => Ok(()),
        }
    }
}

impl Mount {
    /// Hold off the watcher while the daemon writes files itself.
    pub(crate) async fn quiesce(&self) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        match self.watch.get() {
            Some(watch) => Some(watch.quiesce().await),
            None => None,
        }
    }
}
