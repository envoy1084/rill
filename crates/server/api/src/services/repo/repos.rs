//! Repositories: get, list, create, and resolving revisions.

use rill_policy::{Relation, Subject};
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::TrunkSeq;

use super::RepoSvc;
use crate::auth::caller;
use crate::listing::Page;
use crate::services::common::{self, open_repo};
use crate::services::{names, revision, Call};
use rill_runtime::error::{Error, Result};

impl RepoSvc {
    pub(super) async fn get_repo_impl(
        &self,
        request: Call<pb::GetRepoRequest>,
    ) -> Result<pb::GetRepoResponse> {
        names::parse(&rill_proto::names::REPO, &request.get_ref().name)?;
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        opened.permit().read_any().await?;
        Ok(pb::GetRepoResponse {
            repo: common::repo_proto(&opened.repo).await?.into(),
        })
    }

    /// The org's repositories the caller may read, by repository id.
    pub(super) async fn list_repos_impl(
        &self,
        request: Call<pb::ListReposRequest>,
    ) -> Result<pb::ListReposResponse> {
        let ctx = caller(&request)?;
        let req = request.get_ref();
        let org = names::parse(&rill_proto::names::ORG, &req.parent)?[0];
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        let mut configs: Vec<_> = self
            .state
            .repos
            .list()
            .await?
            .into_iter()
            .filter(|c| c.org == org)
            .collect();
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        let mut readable = Vec::new();
        for config in configs {
            let repo = self.state.repos.get(config.id).await?;
            let opened = common::opened(&self.state, ctx.clone(), repo);
            if opened.permit().read_any().await.is_ok() {
                readable.push(opened.repo);
            }
        }
        let (repos, next_page_token) = page.of(readable)?;
        let mut out = Vec::with_capacity(repos.len());
        for repo in repos {
            out.push(common::repo_proto(&repo).await?);
        }
        Ok(pb::ListReposResponse {
            repos: out,
            next_page_token,
        })
    }

    /// Any authenticated principal may create a repository and becomes
    /// its owner, which also takes the repository out of the open
    /// bootstrap state (see `auth::permit`).
    pub(super) async fn create_repo_impl(
        &self,
        request: Call<pb::CreateRepoRequest>,
    ) -> Result<pb::CreateRepoResponse> {
        let ctx = caller(&request)?;
        let req = request.get_ref();
        let org = names::parse(&rill_proto::names::ORG, &req.parent)?[0];
        if req.repo_id.is_empty() || req.repo_id.contains('/') {
            return Err(Error::Convert(rill_proto::ConvertError::Unknown {
                field: "repo_id",
                value: req.repo_id.clone(),
            }));
        }
        if req.repo_id.parse::<rill_types::RepoId>().is_ok() {
            return Err(Error::invalid("repo_id must be a name, not an id"));
        }
        let settings = req.repo.as_option();
        let repo = self
            .state
            .repos
            .create(
                &format!("{org}/{}", req.repo_id),
                settings
                    .and_then(|r| r.description.as_deref())
                    .unwrap_or(""),
                rill_store::RepoSettings {
                    windows_safe: settings.is_some_and(|r| r.windows_safe),
                },
                &ctx.principal,
            )
            .await?;
        repo.authorizer
            .grant(
                rill_policy::rebac::REPO_OBJECT,
                Relation::Owner,
                &Subject::principal(&ctx.principal),
                &ctx.principal,
            )
            .await?;
        tracing::info!(repo = %names::repo(&repo), owner = %ctx.principal, "repository created");
        Ok(pb::CreateRepoResponse {
            repo: common::repo_proto(&repo).await?.into(),
        })
    }

    pub(super) async fn resolve_revision_impl(
        &self,
        request: Call<pb::ResolveRevisionRequest>,
    ) -> Result<pb::ResolveRevisionResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let ctx = &opened.repo;
        let resolved = revision::resolve(ctx, &request.get_ref().revision).await?;
        let trunk_position = match &resolved.version {
            Some(v) => ctx.repo.trunk_seq_of(v).await?.map(TrunkSeq::get),
            None => None,
        };
        Ok(pb::ResolveRevisionResponse {
            version: resolved
                .version
                .map(|v| names::version(ctx, &v))
                .unwrap_or_default(),
            version_hash: resolved
                .version
                .map(|v| hash_to_proto(&v))
                .unwrap_or_default(),
            tree_hash: hash_to_proto(&resolved.root),
            trunk_position,
        })
    }
}
