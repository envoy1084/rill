//! Lazily built per-repository service graphs.
//!
//! A [`RepoContext`] wires the lower crates together for one repository:
//! trees, derived data, the commit graph, review, landing, verification,
//! search and authorization, plus the event bus and background workers.
//! Contexts are built on first use and cached for the life of the process.

use std::sync::Arc;

use arc_swap::ArcSwap;
use dashmap::DashMap;
use rill_core::tree::DEFAULT_TREE_CACHE;
use rill_core::{CommitGraph, DeriveManager, TreeOps};
use rill_exec::{LocalSandbox, Runner};
use rill_lander::{AlwaysGreen, Lander, PostLandVerifier, Verifier};
use rill_policy::Authorizer;
use rill_review::service::{ReviewEvent, ReviewService};
use rill_review::votes::LabelSet;
use rill_search::{IndexConfig, SearchIndex};
use rill_store::repo::RepoConfig;
use rill_store::{RepoStore, Stores};
use rill_types::RepoId;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::policy;
use crate::config::ServerConfig;
use crate::error::{Error, Result};
use crate::workers;

/// How long a search waits for a root it asked to have indexed.
const INDEX_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
/// How often a waiting search looks for the root's manifest.
const INDEX_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Size of the derived-data cache per repository.
const DERIVE_CACHE: usize = 8192;

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
    /// Post-land verifier, when enabled.
    pub postland: Option<Arc<PostLandVerifier>>,
    /// Verification runner, when pre-land verification is enabled.
    pub runner: Option<Arc<Runner>>,
    /// Trigram search index, updated as changes land.
    pub search: Arc<SearchIndex>,
    /// ReBAC relation checks.
    pub authorizer: Arc<Authorizer>,
    /// Vote labels.
    pub labels: Arc<LabelSet>,
    /// Cancelled when the server shuts down; long-lived streams and
    /// workers over this repository watch it.
    pub shutdown: CancellationToken,
    /// The process's draining and abort signals and task tracker.
    pub lifecycle: Arc<super::Lifecycle>,
    review: Arc<ArcSwap<ReviewService>>,
    review_events: broadcast::Sender<ReviewEvent>,
    server_config: Arc<ServerConfig>,
    /// Signs an attestation for every committed version.
    pub attestor: Arc<rill_policy::Signer>,
    index_requests: super::index_requests::IndexRequests,
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

async fn build_review(
    repo: &RepoStore,
    derive: &Arc<DeriveManager>,
    graph: &Arc<CommitGraph>,
    labels: &Arc<LabelSet>,
    config: &ServerConfig,
    events: broadcast::Sender<ReviewEvent>,
) -> Result<ReviewService> {
    let submit = policy::effective_submit(repo).await?;
    let risk = policy::effective_risk(repo, &config.risk).await?;
    Ok(ReviewService::new(
        repo.clone(),
        derive.clone(),
        graph.clone(),
        Arc::new(submit),
        Arc::new(risk),
        labels.clone(),
    )
    .with_events(events))
}

/// Builds and caches [`RepoContext`]s, resolving the `repo` field of
/// requests by name or id.
pub struct RepoRegistry {
    stores: Stores,
    config: Arc<ServerConfig>,
    attestor: Arc<rill_policy::Signer>,
    identity: Arc<crate::identity::IdentityView>,
    contexts: DashMap<RepoId, Arc<RepoContext>>,
    build_lock: tokio::sync::Mutex<()>,
    lifecycle: Arc<super::Lifecycle>,
}

impl RepoRegistry {
    /// A registry over `stores`. Workers spawned for a context stop when
    /// the lifecycle stops.
    pub fn new(
        stores: Stores,
        config: Arc<ServerConfig>,
        attestor: Arc<rill_policy::Signer>,
        identity: Arc<crate::identity::IdentityView>,
        lifecycle: Arc<super::Lifecycle>,
    ) -> Self {
        RepoRegistry {
            stores,
            config,
            attestor,
            identity,
            contexts: DashMap::new(),
            build_lock: tokio::sync::Mutex::new(()),
            lifecycle,
        }
    }

    /// Every repository context built so far.
    pub fn loaded(&self) -> Vec<Arc<RepoContext>> {
        self.contexts.iter().map(|e| e.value().clone()).collect()
    }

    /// The underlying stores.
    pub fn stores(&self) -> &Stores {
        &self.stores
    }

    /// Resolve a request's `repo` field: `org/name`, a bare name in the
    /// default org, or an id in `rp_<hex>` or bare hex form.
    pub async fn resolve(&self, spec: &str) -> Result<Arc<RepoContext>> {
        if spec.is_empty() {
            return Err(Error::invalid("repo is required"));
        }
        let (org, name) = split_repo(spec);
        if let Some(found) = self.stores.org(org).get_repo_by_name(name).await? {
            return self.get(found.id).await;
        }
        if let Ok(id) = spec.parse::<RepoId>() {
            if self.stores.find_repo(&id).await?.is_some() {
                return self.get(id).await;
            }
        }
        Err(Error::not_found(format!("repository {spec:?}")))
    }

    /// The context for `id`, building it on first use.
    pub async fn get(&self, id: RepoId) -> Result<Arc<RepoContext>> {
        if let Some(ctx) = self.contexts.get(&id) {
            return Ok(ctx.clone());
        }
        // The lock serialises builds so two concurrent first requests do
        // not each spawn a lander for the same repository.
        let _guard = self.build_lock.lock().await;
        if let Some(ctx) = self.contexts.get(&id) {
            return Ok(ctx.clone());
        }
        let config = self
            .stores
            .find_repo(&id)
            .await?
            .ok_or_else(|| Error::not_found(format!("repository {id}")))?;
        let ctx = Arc::new(self.build(config).await?);
        workers::spawn_repo_workers(&ctx, self.config.roles);
        self.contexts.insert(id, ctx.clone());
        Ok(ctx)
    }

    /// Create a repository (`org/name`, or a name in the default org) and
    /// build its context.
    pub async fn create(
        &self,
        spec: &str,
        description: &str,
        settings: rill_store::RepoSettings,
        actor: &rill_types::Principal,
    ) -> Result<Arc<RepoContext>> {
        let (org, name) = split_repo(spec);
        let config = self
            .stores
            .org(org)
            .create_repo_with(name, description, settings, actor)
            .await
            .map_err(|e| {
                if e.is_conflict() {
                    Error::AlreadyExists(format!("repository {name:?} already exists"))
                } else {
                    e.into()
                }
            })?;
        self.get(config.id).await
    }

    /// Every repository record, in every org.
    pub async fn list(&self) -> Result<Vec<RepoConfig>> {
        let mut out = Vec::new();
        for org in self.stores.list_orgs().await? {
            out.extend(self.stores.org(&org.name).list_repos().await?);
        }
        Ok(out)
    }

    /// Build every existing repository's context up front so landers and
    /// verifiers run before the first request arrives.
    pub async fn warm(&self) -> Result<usize> {
        let repos = self.list().await?;
        for repo in &repos {
            self.get(repo.id).await?;
        }
        Ok(repos.len())
    }

    async fn build(&self, config: RepoConfig) -> Result<RepoContext> {
        let repo = self.stores.org(&config.org).repo(config.id);
        let trees = TreeOps::new(repo.objects().clone(), DEFAULT_TREE_CACHE);
        let derive = DeriveManager::new(repo.clone(), trees.clone(), DERIVE_CACHE);
        let graph = Arc::new(CommitGraph::new(repo.clone(), derive.clone()));
        let labels = Arc::new(self.config.labels.clone());
        let (review_events, _) = broadcast::channel(rill_review::service::DEFAULT_EVENT_CAPACITY);
        let review = Arc::new(ArcSwap::from_pointee(
            build_review(
                &repo,
                &derive,
                &graph,
                &labels,
                &self.config,
                review_events.clone(),
            )
            .await?,
        ));

        let runner = self.config.verify.enabled.then(|| {
            let sandbox = LocalSandbox::new(self.config.verify.network);
            Arc::new(Runner::new(repo.clone(), trees.clone(), Arc::new(sandbox)))
        });
        let verifier: Arc<dyn Verifier> = match &runner {
            Some(runner) => Arc::new(workers::ExecVerifier::new(runner.clone())),
            None => Arc::new(AlwaysGreen),
        };
        let hooks = Arc::new(
            workers::ReviewHooksAdapter::new(review.clone())
                .with_standing(config.org.clone(), self.identity.clone()),
        );
        let lander = Lander::new(
            repo.clone(),
            derive.clone(),
            graph.clone(),
            hooks,
            verifier,
            self.config.lander.clone(),
        );
        let postland = self
            .config
            .postland
            .enabled
            .then(|| PostLandVerifier::new(lander.clone(), self.config.postland.batch_size));
        let search =
            SearchIndex::open(repo.clone(), derive.clone(), IndexConfig::default()).await?;
        let authorizer = Arc::new(Authorizer::new(repo.clone()));

        Ok(RepoContext {
            config,
            repo,
            trees,
            derive,
            graph,
            lander,
            postland,
            runner,
            search,
            authorizer,
            labels,
            shutdown: self.lifecycle.stop.child_token(),
            lifecycle: self.lifecycle.clone(),
            review,
            review_events,
            server_config: self.config.clone(),
            attestor: self.attestor.clone(),
            index_requests: Default::default(),
        })
    }
}

/// Split `org/name` into its parts; a bare name is in the default org.
pub fn split_repo(spec: &str) -> (&str, &str) {
    match spec.split_once('/') {
        Some((org, name)) => (org, name),
        None => (rill_store::DEFAULT_ORG, spec),
    }
}
