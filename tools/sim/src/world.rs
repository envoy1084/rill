//! The system under test: Rill's real stores, review service and lander,
//! wired as the server wires them, over fault-injecting stores.

use std::sync::Arc;

use arc_swap::ArcSwap;
use bytes::Bytes;
use rill_core::derive::RootTree;
use rill_core::{read_content, CommitGraph, DeriveManager, TreeOps};
use rill_lander::{AlwaysGreen, LandPolicy, Lander};
use rill_policy::{RiskConfig, SubmitPolicy};
use rill_review::{LabelSet, ReviewService};
use rill_runtime::config::IdentityConfig;
use rill_runtime::identity::IdentityView;
use rill_runtime::workers::ReviewHooksAdapter;
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeVersionHash, EntryKind, RepoPath};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::faults::{FaultPlan, FaultyMetaStore, FaultyObjectStore, PartitionedMetaStore};

/// Server processes the world runs: the main one (the API agents talk to,
/// with its lander) and a rival with its own lander, competing for the
/// lease.
pub const PROCESSES: usize = 2;

/// One repository and the services over it.
pub struct World {
    pub repo: RepoStore,
    pub derive: Arc<DeriveManager>,
    pub review: Arc<ArcSwap<ReviewService>>,
    pub lander: Arc<Lander>,
    /// The rival process's lander, over its own view of the stores.
    pub rival: Arc<Lander>,
    pub plan: Arc<FaultPlan>,
    /// Per process: cut off from the metadata store, and until which step.
    cut: [Arc<AtomicBool>; PROCESSES],
    cut_until: [AtomicUsize; PROCESSES],
    /// The lease a lander held when its process was restarted: what a
    /// paused process that wakes up after being replaced still has.
    pub zombie: Option<rill_store::repo::Lease>,
    /// Instance identity, over stores without faults: landers ask it
    /// whether a change's author may still land.
    pub identity: Arc<IdentityView>,
    /// The repository's org.
    pub org: String,
    /// The repository without faults, for the oracle's own reads.
    pub truth: RepoStore,
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
        let cut: [Arc<AtomicBool>; PROCESSES] = Default::default();
        let view = |cut: &Arc<AtomicBool>| {
            stores
                .with_meta(Arc::new(PartitionedMetaStore::new(
                    stores.meta.clone(),
                    cut.clone(),
                )))
                .repo(config.id)
        };
        let identity = IdentityView::open(&clean, &IdentityConfig::default())
            .await
            .expect("identity opens before faults are enabled");
        let standing = (config.org.clone(), identity.clone());
        let repo = view(&cut[0]);
        let (derive, review, lander) = services(&repo, &standing);
        let (_, _, rival) = services(&view(&cut[1]), &standing);
        World {
            repo,
            derive,
            review,
            lander,
            rival,
            plan,
            cut,
            cut_until: Default::default(),
            zombie: None,
            identity,
            org: config.org.clone(),
            truth: clean.repo(config.id),
        }
    }

    /// Cut `process` off from the metadata store until `step`.
    pub fn partition(&self, process: usize, until: usize) {
        self.cut[process].store(true, Ordering::SeqCst);
        self.cut_until[process].store(until, Ordering::SeqCst);
    }

    /// Reconnect processes whose partition is over at `step`; every one
    /// when `step` is `None`.
    pub fn heal(&self, step: Option<usize>) {
        for p in 0..PROCESSES {
            if step.is_none_or(|s| s >= self.cut_until[p].load(Ordering::SeqCst)) {
                self.cut[p].store(false, Ordering::SeqCst);
            }
        }
    }

    pub fn is_partitioned(&self, process: usize) -> bool {
        self.cut[process].load(Ordering::SeqCst)
    }

    /// Simulate a process restart: every in-memory structure (caches, the
    /// land queue, tickets) is rebuilt; only the stores survive.
    pub async fn restart(&mut self) {
        if let Some(lease) = self.lander.held_lease().await {
            self.zombie = Some(lease);
        }
        let standing = (self.org.clone(), self.identity.clone());
        let (derive, review, lander) = services(&self.repo, &standing);
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

fn services(
    repo: &RepoStore,
    standing: &(String, Arc<IdentityView>),
) -> (Arc<DeriveManager>, Arc<ArcSwap<ReviewService>>, Arc<Lander>) {
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
        Arc::new(
            ReviewHooksAdapter::new(review.clone())
                .with_standing(standing.0.clone(), standing.1.clone()),
        ),
        Arc::new(AlwaysGreen),
        LandPolicy::default(),
    );
    (derive, review, lander)
}
