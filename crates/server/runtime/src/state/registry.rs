//! Lazily built per-repository service graphs.
//!
//! A [`RepoContext`] wires the lower crates together for one repository:
//! trees, derived data, the commit graph, review, landing, verification,
//! search and authorization, plus the event bus and background workers.
//! Contexts are built on first use and cached for the life of the process.
use std::sync::Arc;

use dashmap::DashMap;
use rill_store::repo::RepoConfig;
use rill_store::Stores;
use rill_types::RepoId;

use super::RepoContext;
use crate::config::ServerConfig;
use crate::error::{Error, Result};
use crate::workers;

/// Builds and caches [`RepoContext`]s, resolving the `repo` field of
/// requests by name or id.
pub struct RepoRegistry {
    pub(super) stores: Stores,
    pub(super) config: Arc<ServerConfig>,
    pub(super) attestor: Arc<rill_policy::Signer>,
    pub(super) identity: Arc<crate::identity::IdentityView>,
    pub(super) denials: Arc<crate::audit::denials::Denials>,
    pub(super) contexts: DashMap<RepoId, Arc<RepoContext>>,
    pub(super) build_lock: tokio::sync::Mutex<()>,
    pub(super) lifecycle: Arc<super::Lifecycle>,
    /// Structural merge, shared by every repository; `None` when off.
    pub(super) merger: Option<Arc<rill_merge_syntax::WeaveMerger>>,
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
        let merger = config
            .merge
            .structural
            .then(|| Arc::new(rill_merge_syntax::WeaveMerger::new(&config.merge)));
        RepoRegistry {
            denials: Arc::new(crate::audit::denials::Denials::new(stores.audit())),
            stores,
            config,
            attestor,
            identity,
            contexts: DashMap::new(),
            build_lock: tokio::sync::Mutex::new(()),
            lifecycle,
            merger,
        }
    }

    /// The structural merger, when structural merge is on.
    pub fn merger(&self) -> Option<&Arc<rill_merge_syntax::WeaveMerger>> {
        self.merger.as_ref()
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
}

/// Split `org/name` into its parts; a bare name is in the default org.
pub fn split_repo(spec: &str) -> (&str, &str) {
    match spec.split_once('/') {
        Some((org, name)) => (org, name),
        None => (rill_store::DEFAULT_ORG, spec),
    }
}
