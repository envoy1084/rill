//! Wiring the lower crates into a [`RepoContext`] for one repository.

use std::sync::Arc;

use arc_swap::ArcSwap;
use rill_core::tree::DEFAULT_TREE_CACHE;
use rill_core::{CommitGraph, DeriveManager, TreeOps};
use rill_exec::history::TestHistory;
use rill_exec::runs::RunStore;
use rill_exec::PlanRunner;
use rill_lander::{AlwaysGreen, Lander, PostLandVerifier, Verifier};

use super::ReleaseLanders;
use rill_policy::Authorizer;
use rill_review::service::{ReviewEvent, ReviewService};
use rill_review::votes::LabelSet;
use rill_search::{IndexConfig, SearchIndex};
use rill_store::repo::RepoConfig;
use rill_store::RepoStore;
use tokio::sync::broadcast;

use super::{policy, RepoContext, RepoRegistry};
use crate::config::ServerConfig;
use crate::error::Result;
use crate::workers;

/// Size of the derived-data cache per repository.
const DERIVE_CACHE: usize = 8192;

pub(super) async fn build_review(
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

impl RepoRegistry {
    pub(super) async fn build(&self, config: RepoConfig) -> Result<RepoContext> {
        let org = self.stores.org(&config.org);
        let redacting = Arc::new(rill_store::redact::RedactingObjectStore::new(
            org.objects.clone(),
            &config.org,
            self.identity.clone(),
        ));
        let repo = org.with_objects(redacting).repo(config.id);
        let trees = TreeOps::new(repo.objects().clone(), DEFAULT_TREE_CACHE);
        let derive = DeriveManager::new(repo.clone(), trees.clone(), DERIVE_CACHE);
        if let Some(merger) = &self.merger {
            derive.set_merger(merger.clone());
        }
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
            Arc::new(
                PlanRunner::new(
                    self.exec.clone(),
                    &config.org,
                    &config.name,
                    trees.clone(),
                    self.config.verify.platform.clone(),
                )
                .with_env(self.config.verify.env.clone())
                .with_quarantine(TestHistory::new(repo.clone()))
                .with_runs(RunStore::new(
                    repo.clone(),
                    std::time::Duration::from_secs(self.config.verify.retention_days * 24 * 3600),
                )),
            )
        });
        let verifier: Arc<dyn Verifier> = match &runner {
            Some(runner) => Arc::new(workers::ExecVerifier::new(
                runner.clone(),
                TestHistory::new(repo.clone()),
                self.config.verify.flakes,
            )),
            None => Arc::new(AlwaysGreen),
        };
        let authorizer = Arc::new(Authorizer::new(repo.clone()));
        let mut hooks = workers::ReviewHooksAdapter::new(review.clone())
            .with_standing(config.org.clone(), self.identity.clone())
            .with_graph(graph.clone());
        if !self.config.dev_mode {
            hooks = hooks.with_write_rights(workers::rights::WriteRights {
                org: config.org.clone(),
                authorizer: authorizer.clone(),
                identity: self.identity.clone(),
                trees: trees.clone(),
                derive: derive.clone(),
            });
        }
        let hooks = Arc::new(hooks);
        let build = {
            let (repo, derive, graph) = (repo.clone(), derive.clone(), graph.clone());
            let policy = self.config.lander.clone();
            move |line: rill_types::Line| {
                let mut lander = Lander::new(
                    repo.clone(),
                    derive.clone(),
                    graph.clone(),
                    hooks.clone(),
                    verifier.clone(),
                    policy.clone(),
                )
                .on_line(line);
                if let Some(runner) = &runner {
                    lander = lander
                        .with_regenerator(Arc::new(workers::ExecRegenerator::new(runner.clone())))
                        .with_targets(Arc::new(workers::ExecTargetGraph::new(runner.clone())));
                }
                lander
            }
        };
        let lander = Arc::new(build(rill_types::Line::Trunk));
        let releases = Arc::new(ReleaseLanders::new(repo.clone(), build));
        let postland = self.config.postland.enabled.then(|| {
            PostLandVerifier::with_full_every(
                lander.clone(),
                self.config.postland.batch_size,
                std::time::Duration::from_secs(self.config.postland.full_every_secs),
            )
        });
        let search =
            SearchIndex::open(repo.clone(), derive.clone(), IndexConfig::default()).await?;

        Ok(RepoContext {
            config,
            repo,
            trees,
            derive,
            graph,
            lander,
            releases,
            postland,
            search,
            authorizer,
            identity: self.identity.clone(),
            denials: self.denials.clone(),
            secrets: Default::default(),
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
