//! Repository records, refs, trunk and derived-data RPCs.

use rill_policy::{Relation, Subject};
use rill_proto::convert::{hash_to_proto, ref_name_from_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ObjectHash, RefUpdate, RepoPathBuf, Timestamp, TrunkSeq};

use super::RepoSvc;
use crate::auth::caller;
use crate::error::{Error, Result};
use crate::services::common::{self, hash, open};
use crate::state::RepoEvent;

/// Longest a `WaitDerived` call blocks.
const MAX_WAIT_MS: u64 = 60_000;
const WAIT_POLL: std::time::Duration = std::time::Duration::from_millis(50);

impl RepoSvc {
    pub(super) async fn get_repo_impl(
        &self,
        request: tonic::Request<pb::GetRepoRequest>,
    ) -> Result<pb::GetRepoResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        Ok(pb::GetRepoResponse {
            repo: Some(common::repo_proto(&opened.repo).await?),
        })
    }

    pub(super) async fn list_repos_impl(
        &self,
        request: tonic::Request<pb::ListReposRequest>,
    ) -> Result<pb::ListReposResponse> {
        let ctx = caller(&request)?;
        let limit = common::limit(request.get_ref().limit, 100, 10_000);
        let mut repos = Vec::new();
        for config in self.state.repos.list().await? {
            let repo = self.state.repos.get(config.id).await?;
            let opened = common::opened(&self.state, ctx.clone(), repo);
            if opened.permit().read(&RepoPathBuf::root()).await.is_ok() {
                repos.push(common::repo_proto(&opened.repo).await?);
            }
            if repos.len() >= limit {
                break;
            }
        }
        Ok(pb::ListReposResponse {
            repos,
            next_page_token: None,
        })
    }

    /// Any authenticated principal may create a repository and becomes
    /// its owner, which also takes the repository out of the open
    /// bootstrap state (see `auth::permit`).
    pub(super) async fn create_repo_impl(
        &self,
        request: tonic::Request<pb::CreateRepoRequest>,
    ) -> Result<pb::CreateRepoResponse> {
        let ctx = caller(&request)?;
        let req = request.get_ref();
        if req.repo.is_empty()
            || req.repo.contains('/')
            || req.repo.parse::<rill_types::RepoId>().is_ok()
        {
            return Err(Error::invalid("repo must be a non-empty name without '/'"));
        }
        let repo = self
            .state
            .repos
            .create(
                &req.repo,
                req.description.as_deref().unwrap_or(""),
                rill_store::RepoSettings {
                    windows_safe: req.windows_safe,
                },
            )
            .await?;
        repo.authorizer
            .grant(
                rill_policy::rebac::REPO_OBJECT,
                Relation::Owner,
                &Subject::principal(&ctx.principal),
            )
            .await?;
        tracing::info!(repo = %req.repo, owner = %ctx.principal, "repository created");
        Ok(pb::CreateRepoResponse {
            repo: Some(common::repo_proto(&repo).await?),
        })
    }

    pub(super) async fn resolve_ref_impl(
        &self,
        request: tonic::Request<pb::ResolveRefRequest>,
    ) -> Result<pb::ResolveRefResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let name = ref_name_from_proto("name", &request.get_ref().name)?;
        let hash = opened
            .repo
            .repo
            .get_ref(&name)
            .await?
            .ok_or_else(|| Error::not_found(format!("ref {name:?}")))?;
        let trunk_seq = if name.is_trunk() {
            opened
                .repo
                .repo
                .trunk_seq_of(&hash)
                .await?
                .map(TrunkSeq::get)
        } else {
            None
        };
        Ok(pb::ResolveRefResponse {
            hash: hash_to_proto(&hash),
            trunk_seq,
        })
    }

    pub(super) async fn list_refs_impl(
        &self,
        request: tonic::Request<pb::ListRefsRequest>,
    ) -> Result<pb::ListRefsResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let prefix = request.get_ref().prefix.clone().unwrap_or_default();
        let mut refs = Vec::new();
        for (name, target) in opened.repo.repo.list_refs().await? {
            if !name.as_str().starts_with(&prefix) {
                continue;
            }
            let trunk_seq = if name.is_trunk() {
                opened
                    .repo
                    .repo
                    .trunk_seq_of(&target)
                    .await?
                    .map(TrunkSeq::get)
            } else {
                None
            };
            refs.push(pb::Ref {
                name: name.to_string(),
                target: hash_to_proto(&target),
                trunk_seq,
            });
        }
        Ok(pb::ListRefsResponse { refs })
    }

    /// Compare-and-swap a ref. Trunk is owned by the lander: it only moves
    /// through landings, so requests to move it are refused.
    pub(super) async fn move_ref_impl(
        &self,
        request: tonic::Request<pb::MoveRefRequest>,
    ) -> Result<pb::MoveRefResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().write(&RepoPathBuf::root()).await?;
        let update = RefUpdate::try_from(
            request
                .get_ref()
                .update
                .clone()
                .ok_or_else(|| Error::invalid("update is required"))?,
        )?;
        if update.name.is_trunk() {
            return Err(Error::denied(
                "trunk only moves through landings; use RequestLand",
            ));
        }
        if let Some(target) = &update.target {
            if !opened.repo.repo.has_change_version(target).await? {
                return Err(Error::precondition(format!(
                    "target {target} does not exist"
                )));
            }
        }
        opened
            .repo
            .repo
            .update_ref(&update.name, update.expected, update.target)
            .await
            .map_err(|e| {
                if e.is_conflict() {
                    Error::FailedPrecondition(format!(
                        "ref {:?} is not at the expected target",
                        update.name
                    ))
                } else {
                    e.into()
                }
            })?;
        opened.repo.events.publish(RepoEvent::RefMoved {
            name: update.name.clone(),
            old: update.expected,
            new: update.target,
            at: Timestamp::now(),
        });
        Ok(pb::MoveRefResponse {
            hash: update.target.map(|h| hash_to_proto(&h)),
        })
    }

    pub(super) async fn trunk_head_impl(
        &self,
        request: tonic::Request<pb::TrunkHeadRequest>,
    ) -> Result<pb::TrunkHeadResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let head = opened
            .repo
            .repo
            .trunk_head()
            .await?
            .ok_or_else(|| Error::not_found("trunk has no head yet"))?;
        Ok(pb::TrunkHeadResponse {
            seq: head.0.get(),
            hash: hash_to_proto(&head.1),
        })
    }

    pub(super) async fn trunk_range_impl(
        &self,
        request: tonic::Request<pb::TrunkRangeRequest>,
    ) -> Result<pb::TrunkRangeResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        if req.to < req.from {
            return Err(Error::invalid("to must not be less than from"));
        }
        let mut landings = Vec::new();
        for (seq, hash) in opened
            .repo
            .repo
            .trunk_range(TrunkSeq(req.from.saturating_sub(1)), TrunkSeq(req.to))
            .await?
        {
            landings.push(common::land_event(&opened.repo, seq, hash).await?);
        }
        Ok(pb::TrunkRangeResponse { landings })
    }

    pub(super) async fn get_derived_impl(
        &self,
        request: tonic::Request<pb::GetDerivedRequest>,
    ) -> Result<pb::GetDerivedResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        let hash = hash("hash", &req.hash)?;
        match derived(&opened.repo, &req.kind, &hash, true).await? {
            Some(data) => Ok(pb::GetDerivedResponse { data }),
            None => Err(Error::not_found(format!(
                "derived {:?} for {hash}",
                req.kind
            ))),
        }
    }

    pub(super) async fn wait_derived_impl(
        &self,
        request: tonic::Request<pb::WaitDerivedRequest>,
    ) -> Result<pb::WaitDerivedResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        let hash = hash("hash", &req.hash)?;
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(req.timeout_ms.min(MAX_WAIT_MS));
        loop {
            if let Some(data) = derived(&opened.repo, &req.kind, &hash, true).await? {
                return Ok(pb::WaitDerivedResponse {
                    ready: true,
                    data: Some(data),
                });
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(pb::WaitDerivedResponse {
                    ready: false,
                    data: None,
                });
            }
            tokio::time::sleep(WAIT_POLL).await;
        }
    }
}

/// Fetch a derived record's raw bytes. Known framework kinds are computed
/// on demand when `compute` is set; other kinds are read as stored.
async fn derived(
    ctx: &crate::state::RepoContext,
    kind: &str,
    hash: &ObjectHash,
    compute: bool,
) -> Result<Option<bytes::Bytes>> {
    if compute && ctx.repo.has_change_version(hash).await? {
        ctx.derive.compute_kind(kind, hash).await?;
    }
    Ok(ctx.repo.get_derived(kind, hash).await?)
}
