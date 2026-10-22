//! A scheduler with one local worker over in-memory stores.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rill_core::tree::{Edit, TreeBuilder};
use rill_core::{store_content, TreeOps};
use rill_exec::remote::{Scheduler, SchedulerConfig};
use rill_exec::{LocalSandbox, LocalWorker, PlanRunner, Target, VerificationOutcome, PLAN_PATH};
use rill_jobs::Priority;
use rill_store::Stores;
use rill_types::{FileKind, RepoPathBuf, TreeHash};
use tokio::task::JoinHandle;

pub struct Fixture {
    pub repo: rill_store::RepoStore,
    pub trees: TreeOps,
    pub runs: rill_exec::runs::RunStore,
    pub runner: PlanRunner,
    pub scheduler: Arc<Scheduler>,
    worker: JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

impl Fixture {
    pub async fn new() -> Self {
        Self::build(Arc::new(rill_store::Plaintext), 0.0).await
    }

    /// A fixture whose scheduler seals secrets and results with `sealer`.
    pub async fn sealed_by(sealer: Arc<dyn rill_store::Sealer>) -> Self {
        Self::build(sealer, 0.0).await
    }

    /// A fixture that probes `probe_rate` of the results it caches.
    pub async fn probing(probe_rate: f64) -> Self {
        Self::build(Arc::new(rill_store::Plaintext), probe_rate).await
    }

    async fn build(sealer: Arc<dyn rill_store::Sealer>, probe_rate: f64) -> Self {
        let stores = Stores::memory();
        let repo = stores.repo(stores.create_repo("app", "").await.unwrap().id);
        let trees = TreeOps::new(repo.objects().clone(), 256);
        let scheduler = Arc::new(
            Scheduler::new(
                stores.clone(),
                SchedulerConfig {
                    idle_wait: Duration::from_millis(200),
                    report_every: Duration::from_millis(500),
                    probe_rate,
                    ..SchedulerConfig::default()
                },
            )
            .with_sealer(sealer),
        );
        let worker = LocalWorker::new(
            scheduler.clone(),
            Arc::new(LocalSandbox::new(false)),
            "test-worker",
            &Default::default(),
        );
        let worker = tokio::spawn(worker.run(std::future::pending()));
        let runs =
            rill_exec::runs::RunStore::new(repo.clone(), Duration::from_secs(30 * 24 * 3600));
        let runner = PlanRunner::new(
            scheduler.clone(),
            rill_store::DEFAULT_ORG,
            "app",
            trees.clone(),
            Default::default(),
        )
        .with_runs(runs.clone())
        .with_quarantine(rill_exec::history::TestHistory::new(repo.clone()));
        Fixture {
            repo,
            trees,
            runs,
            runner,
            scheduler,
            worker,
        }
    }

    /// `base` with `files` set; `PLAN` names the plan file.
    pub async fn tree(&self, base: Option<TreeHash>, files: &[(&str, &str)]) -> TreeHash {
        let mut builder = TreeBuilder::new();
        for (path, body) in files {
            let path = if *path == "PLAN" { PLAN_PATH } else { path };
            let content = store_content(
                self.trees.objects().as_ref(),
                Bytes::copy_from_slice(body.as_bytes()),
            )
            .await
            .unwrap();
            builder.edit(
                &RepoPathBuf::new(path).unwrap(),
                Edit::Set(content, FileKind::Regular),
            );
        }
        builder.build(&self.trees, base).await.unwrap()
    }

    pub async fn verify(&self, root: TreeHash, base: Option<TreeHash>) -> VerificationOutcome {
        self.verify_at(root, base, false).await
    }

    /// [`Fixture::verify`] as trunk (after landing) or not.
    pub async fn verify_at(
        &self,
        root: TreeHash,
        base: Option<TreeHash>,
        trunk: bool,
    ) -> VerificationOutcome {
        let target = Target {
            root,
            version: rill_types::ObjectHash::ZERO,
            base,
            risk: None,
            priority: Priority::Normal,
            trunk,
        };
        tokio::time::timeout(Duration::from_secs(60), self.runner.verify(&target))
            .await
            .expect("verification finished")
            .unwrap()
    }
}
