//! Everything the handlers need for one repository, and what it serves
//! directly: on-demand search and policy reloads.

use std::sync::Arc;

use arc_swap::ArcSwap;
use rill_core::{CommitGraph, DeriveManager, TreeOps};
use rill_lander::{Lander, PostLandVerifier};
use rill_policy::Authorizer;
use rill_review::service::{ReviewEvent, ReviewService};
use rill_review::votes::LabelSet;
use rill_search::SearchIndex;
use rill_store::repo::RepoConfig;
use rill_store::RepoStore;
use rill_types::RepoId;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::repo_build::build_review;
use crate::config::ServerConfig;
use crate::error::Result;

/// Everything the handlers need for one repository.
pub struct RepoContext {
    /// The repository record.
    pub config: RepoConfig,
    /// Typed access to the repository's keyspace and objects.
    pub repo: RepoStore,
    /// Tree reads and writes with a cache.
    pub trees: TreeOps,
    /// Derived data (root trees, blame, history, summaries).
    pub derive: Arc<DeriveManager>,
    /// Ancestry queries.
    pub graph: Arc<CommitGraph>,
    /// The landing pipeline.
    pub lander: Arc<Lander>,
    /// The landers of the repository's releases.
    pub releases: Arc<super::ReleaseLanders>,
    /// Post-land verifier, when enabled.
    pub postland: Option<Arc<PostLandVerifier>>,
    /// Trigram search index, updated as changes land.
    pub search: Arc<SearchIndex>,
    /// ReBAC relation checks.
    pub authorizer: Arc<Authorizer>,
    /// Users, agents and org roles, for org administrators' access.
    pub identity: Arc<crate::identity::IdentityView>,
    /// Where refused requests are noted.
    pub denials: Arc<crate::audit::denials::Denials>,
    /// Secret scanning rules and what was found clean.
    pub secrets: Arc<crate::secrets::Scanner>,
    /// Vote labels.
    pub labels: Arc<LabelSet>,
    /// Cancelled when the server shuts down; long-lived streams and
    /// workers over this repository watch it.
    pub shutdown: CancellationToken,
    /// The process's draining and abort signals and task tracker.
    pub lifecycle: Arc<super::Lifecycle>,
    pub(super) review: Arc<ArcSwap<ReviewService>>,
    pub(super) review_events: broadcast::Sender<ReviewEvent>,
    /// Teams and sponsors for review.
    pub(super) directory: Arc<dyn rill_policy::Directory>,
    pub(crate) server_config: Arc<ServerConfig>,
    /// The execution scheduler, which also keeps the org's secrets.
    pub exec: Arc<rill_exec::remote::Scheduler>,
    /// Signs an attestation for every committed version.
    pub attestor: Arc<rill_policy::Signer>,
    pub(super) index_requests: super::index_requests::IndexRequests,
    /// Which servers serve which search shards.
    pub search_nodes: Arc<super::search_nodes::SearchNodes>,
}

impl RepoContext {
    /// The repository id.
    pub fn id(&self) -> RepoId {
        self.config.id
    }

    /// Seconds between post-land verification sweeps.
    pub fn postland_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.server_config.postland.interval_secs.max(1))
    }

    /// The current review service. Hot-swapped when policy changes, so
    /// callers must not hold the returned `Arc` across requests.
    pub fn review(&self) -> Arc<ReviewService> {
        self.review.load_full()
    }

    /// Rebuild the review service from the policies stored in the repo
    /// (after `SetPolicy`) and swap it in. Event subscribers are unaffected
    /// because the broadcast sender is shared across rebuilds.
    pub async fn reload_policy(&self) -> Result<()> {
        let service = build_review(
            &self.repo,
            &self.derive,
            &self.graph,
            &self.labels,
            &self.server_config,
            self.review_events.clone(),
            self.directory.clone(),
        )
        .await?;
        self.review.store(Arc::new(service));
        tracing::info!(repo = %self.config.name, "review policy reloaded");
        Ok(())
    }
}
