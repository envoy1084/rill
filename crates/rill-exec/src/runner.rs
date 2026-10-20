//! Cache-aware execution of actions.

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use rill_core::TreeOps;
use rill_store::RepoStore;
use rill_types::{ContentRef, ObjectHash, RepoPathBuf, Timestamp};
use tokio::sync::Mutex;

use crate::action::{Action, ActionResult};
use crate::cache::ActionCache;
use crate::error::{Error, Result};
use crate::materialize::{local_path, materialize, MaterializeOptions};
use crate::sandbox::Sandbox;

/// Whether an execution was served from the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Hit,
    Miss,
}

/// Executes actions, consulting and filling the action cache.
pub struct Runner {
    repo: RepoStore,
    trees: TreeOps,
    cache: ActionCache,
    sandbox: Arc<dyn Sandbox>,
    /// One lock per digest being executed, so concurrent requests for the
    /// same action run it once and the rest read the cached result.
    in_flight: DashMap<ObjectHash, Arc<Mutex<()>>>,
}

impl Runner {
    /// A runner over `repo` executing with `sandbox`.
    pub fn new(repo: RepoStore, trees: TreeOps, sandbox: Arc<dyn Sandbox>) -> Self {
        Runner {
            cache: ActionCache::new(repo.clone()),
            repo,
            trees,
            sandbox,
            in_flight: DashMap::new(),
        }
    }

    /// The repository this runner stores results in.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// Tree operations used to materialize inputs.
    pub fn trees(&self) -> &TreeOps {
        &self.trees
    }

    /// The action cache.
    pub fn cache(&self) -> &ActionCache {
        &self.cache
    }

    /// Run `action`, or return its cached result.
    ///
    /// The same digest is never executed twice: a hit returns the stored
    /// result, and concurrent misses for one digest serialize on it.
    pub async fn execute(&self, action: &Action) -> Result<(ActionResult, CacheStatus)> {
        let digest = action.digest();
        if let Some(result) = self.cache.get(&digest).await? {
            return Ok((result, CacheStatus::Hit));
        }
        let lock = self
            .in_flight
            .entry(digest)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let guard = lock.lock().await;
        let outcome = match self.cache.get(&digest).await? {
            Some(result) => Ok((result, CacheStatus::Hit)),
            None => self.run_uncached(action, &digest).await,
        };
        drop(guard);
        self.in_flight.remove(&digest);
        outcome
    }

    async fn run_uncached(
        &self,
        action: &Action,
        digest: &ObjectHash,
    ) -> Result<(ActionResult, CacheStatus)> {
        let dir = tokio::task::spawn_blocking(tempfile::tempdir).await??;
        materialize(
            &self.trees,
            action.input_root,
            dir.path(),
            &MaterializeOptions::default(),
        )
        .await?;

        let started_at = Timestamp::now();
        tracing::info!(digest = %digest.short(), program = ?action.program(), "executing action");
        let raw = self.sandbox.run(dir.path(), action).await?;

        let output_files = self.collect_outputs(dir.path(), action).await?;
        let result = ActionResult {
            exit_code: raw.exit_code,
            stdout: self.cache.store_output(raw.stdout).await?,
            stderr: self.cache.store_output(raw.stderr).await?,
            duration_ms: raw.duration.as_millis() as u64,
            output_files,
            worker: self.sandbox.worker_name(),
            started_at,
            timed_out: raw.timed_out,
        };
        self.cache.put(digest, &result).await?;
        // Dropping the tempdir removes it; do that off the runtime thread.
        tokio::task::spawn_blocking(move || drop(dir)).await?;
        Ok((result, CacheStatus::Miss))
    }

    /// Read the declared output paths that exist after the run. A missing
    /// output is not an error: the command's exit code says whether it
    /// did its job, and callers see which outputs are absent.
    async fn collect_outputs(
        &self,
        dir: &std::path::Path,
        action: &Action,
    ) -> Result<BTreeMap<RepoPathBuf, ContentRef>> {
        let mut paths = Vec::with_capacity(action.output_paths.len());
        for path in &action.output_paths {
            paths.push((path.clone(), local_path(dir, path)?));
        }
        let read = tokio::task::spawn_blocking(move || {
            let mut out = Vec::with_capacity(paths.len());
            for (repo_path, local) in paths {
                match std::fs::read(&local) {
                    Ok(bytes) => out.push((repo_path, Bytes::from(bytes))),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(Error::Io(e)),
                }
            }
            Ok(out)
        })
        .await??;
        let mut outputs = BTreeMap::new();
        for (path, bytes) in read {
            outputs.insert(path, self.cache.store_output(bytes).await?);
        }
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use rill_core::tree::{Edit, TreeBuilder};
    use rill_store::Stores;
    use rill_types::FileKind;

    use super::*;
    use crate::sandbox::RawOutcome;

    struct CountingSandbox {
        runs: AtomicUsize,
    }

    #[async_trait]
    impl Sandbox for CountingSandbox {
        async fn run(&self, dir: &Path, action: &Action) -> Result<RawOutcome> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            for path in &action.output_paths {
                std::fs::write(local_path(dir, path).unwrap(), b"made").unwrap();
            }
            Ok(RawOutcome {
                exit_code: 0,
                stdout: Bytes::from_static(b"ok"),
                stderr: Bytes::new(),
                duration: Duration::from_millis(1),
                timed_out: false,
            })
        }
    }

    async fn runner() -> (Runner, Arc<CountingSandbox>) {
        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        let repo = stores.repo(config.id);
        let trees = TreeOps::new(repo.objects().clone(), 16);
        let sandbox = Arc::new(CountingSandbox {
            runs: AtomicUsize::new(0),
        });
        (Runner::new(repo, trees, sandbox.clone()), sandbox)
    }

    #[tokio::test]
    async fn cache_hit_skips_execution() {
        let (runner, sandbox) = runner().await;
        let mut b = TreeBuilder::new();
        b.edit(
            &RepoPathBuf::new("f").unwrap(),
            Edit::Set(ContentRef::inline(b"x".to_vec()), FileKind::Regular),
        );
        let root = b.build(runner.trees(), None).await.unwrap();
        let mut action = Action::new(vec!["true".into()], root);
        action
            .output_paths
            .push(RepoPathBuf::new("out.txt").unwrap());

        let (first, status) = runner.execute(&action).await.unwrap();
        assert_eq!(status, CacheStatus::Miss);
        assert_eq!(sandbox.runs.load(Ordering::SeqCst), 1);
        assert_eq!(first.exit_code, 0);
        assert_eq!(first.output_files.len(), 1);
        assert_eq!(
            runner
                .cache()
                .read_output(&first.stdout)
                .await
                .unwrap()
                .as_ref(),
            b"ok"
        );

        let (second, status) = runner.execute(&action).await.unwrap();
        assert_eq!(status, CacheStatus::Hit);
        assert_eq!(second, first);
        assert_eq!(sandbox.runs.load(Ordering::SeqCst), 1);

        let mut other = action.clone();
        other.env.insert("X".into(), "1".into());
        let (_, status) = runner.execute(&other).await.unwrap();
        assert_eq!(status, CacheStatus::Miss);
        assert_eq!(sandbox.runs.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_misses_run_once() {
        let (runner, sandbox) = runner().await;
        let runner = Arc::new(runner);
        let root = runner.trees().empty_tree().await.unwrap();
        let action = Action::new(vec!["true".into()], root);
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let runner = runner.clone();
                let action = action.clone();
                tokio::spawn(async move { runner.execute(&action).await.unwrap() })
            })
            .collect();
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(sandbox.runs.load(Ordering::SeqCst), 1);
    }
}
