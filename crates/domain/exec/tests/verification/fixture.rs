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
    pub trees: TreeOps,
    pub runner: PlanRunner,
    worker: JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

impl Fixture {
    pub async fn new() -> Self {
        let stores = Stores::memory();
        let repo = stores.repo(stores.create_repo("app", "").await.unwrap().id);
        let trees = TreeOps::new(repo.objects().clone(), 256);
        let scheduler = Arc::new(Scheduler::new(
            stores,
            SchedulerConfig {
                idle_wait: Duration::from_millis(200),
                report_every: Duration::from_millis(500),
                ..SchedulerConfig::default()
            },
        ));
        let worker = LocalWorker::new(
            scheduler.clone(),
            Arc::new(LocalSandbox::new(false)),
            "test-worker",
            &Default::default(),
        );
        let worker = tokio::spawn(worker.run(std::future::pending()));
        let runner = PlanRunner::new(
            scheduler,
            rill_store::DEFAULT_ORG,
            trees.clone(),
            Default::default(),
            "test",
        );
        Fixture {
            trees,
            runner,
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
        let target = Target {
            root,
            base,
            risk: None,
            priority: Priority::Normal,
        };
        tokio::time::timeout(Duration::from_secs(60), self.runner.verify(&target))
            .await
            .expect("verification finished")
            .unwrap()
    }
}
