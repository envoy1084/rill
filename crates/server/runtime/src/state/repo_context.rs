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

/// How long a search waits for a root it asked to have indexed.
const INDEX_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
/// How often a waiting search looks for the root's manifest.
const INDEX_POLL: std::time::Duration = std::time::Duration::from_millis(50);

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
    pub(super) server_config: Arc<ServerConfig>,
    /// Signs an attestation for every committed version.
    pub attestor: Arc<rill_policy::Signer>,
    pub(super) index_requests: super::index_requests::IndexRequests,
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

    /// Search an indexed root, showing only paths `allowed` admits. This
    /// is the server side of `RepoService.Grep`. The root is indexed on demand
    /// when the indexer has not caught up with it.
    pub async fn grep(
        &self,
        root: rill_types::TreeHash,
        query: rill_search::SearchQuery,
        allowed: rill_policy::PathFilter,
    ) -> Result<Vec<rill_search::SearchHit>> {
        if self.search.manifest(root).await.is_err() {
            self.wait_for_index(root).await?;
        }
        // Boxed so the borrow-heavy search future has a concrete `Send`
        // bound; otherwise the compiler cannot prove it for the tonic
        // handler that awaits this.
        let search = self.search.clone();
        let fut: std::pin::Pin<
            Box<
                dyn std::future::Future<Output = rill_search::Result<Vec<rill_search::SearchHit>>>
                    + Send,
            >,
        > = Box::pin(async move {
            rill_search::search(&search, root, &query, move |p| allowed.allows(p)).await
        });
        Ok(fut.await?)
    }

    /// Ask the search writer to index `root` and wait for its manifest:
    /// only the server holding the search lease writes the index.
    async fn wait_for_index(&self, root: rill_types::TreeHash) -> Result<()> {
        let _request = match self.index_requests.claim(root) {
            Some(request) => {
                rill_jobs::Queue::new(self.repo.clone(), rill_lander::land::followups::INDEX_QUEUE)
                    .push(rill_lander::land::followups::index_root_job(&root))
                    .await?;
                Some(request)
            }
            None => None,
        };
        let deadline = tokio::time::Instant::now() + INDEX_WAIT;
        while tokio::time::Instant::now() < deadline {
            if self.search.manifest(root).await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(INDEX_POLL).await;
        }
        Err(rill_search::Error::RootNotIndexed(root).into())
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
        )
        .await?;
        self.review.store(Arc::new(service));
        tracing::info!(repo = %self.config.name, "review policy reloaded");
        Ok(())
    }
}
