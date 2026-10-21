//! The system under test: Rill's real stores, review service and lander,
//! wired as the server wires them, over fault-injecting stores.

use std::sync::Arc;

use arc_swap::ArcSwap;
use bytes::Bytes;
use rill_api::workers::ReviewHooksAdapter;
use rill_core::derive::RootTree;
use rill_core::{read_content, CommitGraph, DeriveManager, TreeOps};
use rill_lander::{AlwaysGreen, LandPolicy, Lander};
use rill_policy::{RiskConfig, SubmitPolicy};
use rill_review::{LabelSet, ReviewService};
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeVersionHash, EntryKind, RepoPath};

use crate::faults::{FaultPlan, FaultyMetaStore, FaultyObjectStore};

/// One repository and the services over it.
pub struct World {
    pub repo: RepoStore,
    pub derive: Arc<DeriveManager>,
    pub review: Arc<ArcSwap<ReviewService>>,
    pub lander: Arc<Lander>,
    pub plan: Arc<FaultPlan>,
    /// The lease a lander held when its process was restarted: what a
    /// paused process that wakes up after being replaced still has.
    pub zombie: Option<rill_store::repo::Lease>,
}

impl World {
    /// A fresh repository over memory stores wrapped by `plan`.
    pub async fn new(plan: Arc<FaultPlan>) -> Self {
        let clean = Stores::memory();
        let stores = Stores::new(
            Arc::new(FaultyObjectStore::new(clean.objects.clone(), plan.clone())),
            Arc::new(FaultyMetaStore::new(clean.meta.clone(), plan.clone())),
        );
        let config = stores
            .create_repo("sim", "")
            .await
            .expect("repository creation runs before faults are enabled");
        let repo = stores.repo(config.id);
        let (derive, review, lander) = services(&repo);
        World {
            repo,
            derive,
            review,
            lander,
            plan,
            zombie: None,
        }
    }

    /// Simulate a process restart: every in-memory structure (caches, the
    /// land queue, tickets) is rebuilt; only the stores survive.
    pub async fn restart(&mut self) {
        if let Some(lease) = self.lander.held_lease().await {
            self.zombie = Some(lease);
        }
        let (derive, review, lander) = services(&self.repo);
        self.derive = derive;
        self.review = review;
        self.lander = lander;
    }

    /// The current review service.
    pub fn review(&self) -> Arc<ReviewService> {
        self.review.load_full()
    }

    /// Lines of `path` at `version`; empty when the file is absent.
    pub async fn file_lines(
        &self,
        version: ChangeVersionHash,
        path: &RepoPath,
    ) -> rill_core::Result<Vec<String>> {
        let root = self.derive.get::<RootTree>(&version).await?;
        let bytes = match self.derive.trees().lookup(root, path).await? {
            Some(EntryKind::File { content, .. }) => {
                read_content(self.repo.objects().as_ref(), &content).await?
            }
            _ => Bytes::new(),
        };
        Ok(String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::to_string)
            .collect())
    }
}

fn services(repo: &RepoStore) -> (Arc<DeriveManager>, Arc<ArcSwap<ReviewService>>, Arc<Lander>) {
    let trees = TreeOps::new(repo.objects().clone(), 256);
    let derive = DeriveManager::new(repo.clone(), trees, 256);
    let graph = Arc::new(CommitGraph::new(repo.clone(), derive.clone()));
    let review = Arc::new(ArcSwap::from_pointee(ReviewService::new(
        repo.clone(),
        derive.clone(),
        graph.clone(),
        Arc::new(SubmitPolicy::default_rules()),
        Arc::new(
            RiskConfig::default()
                .compile()
                .expect("the default risk config compiles"),
        ),
        Arc::new(LabelSet::default()),
    )));
    let lander = Lander::new(
        repo.clone(),
        derive.clone(),
        graph,
        Arc::new(ReviewHooksAdapter::new(review.clone())),
        Arc::new(AlwaysGreen),
        LandPolicy::default(),
    );
    (derive, review, lander)
}
