//! Release landers: one per `release/<name>` ref, built on demand with the
//! repository's own services, each under its own lease, so a release moves
//! only through its lander and never blocks trunk.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use rill_lander::Lander;
use rill_store::RepoStore;
use rill_types::Line;
use tokio_util::sync::CancellationToken;

type Build = Box<dyn Fn(Line) -> Lander + Send + Sync>;

/// The landers of a repository's releases.
pub struct ReleaseLanders {
    repo: RepoStore,
    build: Build,
    landers: parking_lot::Mutex<HashMap<String, Arc<Lander>>>,
}

impl ReleaseLanders {
    /// Landers built by `build` for the releases of `repo`.
    pub fn new(repo: RepoStore, build: impl Fn(Line) -> Lander + Send + Sync + 'static) -> Self {
        ReleaseLanders {
            repo,
            build: Box::new(build),
            landers: Default::default(),
        }
    }

    /// The lander of release `name`.
    pub fn get(&self, name: &str) -> Arc<Lander> {
        self.landers
            .lock()
            .entry(name.to_string())
            .or_insert_with(|| Arc::new((self.build)(Line::Release(name.to_string()))))
            .clone()
    }

    /// Serve every release's queue, concurrently, until `shutdown`: after
    /// each sweep, wait for a new release request or `window`.
    pub async fn run_forever(self: Arc<Self>, window: Duration, shutdown: CancellationToken) {
        // Every release shares one wake counter.
        let any = Line::Release(String::new());
        loop {
            let seen = self.repo.land_wake_count(&any).await.unwrap_or(0);
            if let Err(e) = self.sweep().await {
                tracing::error!(error = %e, "release landing sweep failed");
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = self.repo.wait_for_land_requests(&any, seen, window) => {}
            }
        }
        let landers: Vec<Arc<Lander>> = self.landers.lock().values().cloned().collect();
        for lander in landers {
            lander.release().await;
        }
    }

    async fn sweep(&self) -> rill_store::Result<()> {
        let releases: Vec<String> = self
            .repo
            .list_refs()
            .await?
            .into_iter()
            .filter_map(|(name, _)| Line::of_ref(&name)?.release().map(str::to_string))
            .collect();
        let landers: Vec<Arc<Lander>> = releases.iter().map(|name| self.get(name)).collect();
        for result in join_all(landers.iter().map(|l| l.run_once())).await {
            if let Err(e) = result {
                tracing::warn!(error = %e, "serving a release queue failed");
            }
        }
        Ok(())
    }
}
