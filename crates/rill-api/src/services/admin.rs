//! `rill.v1.AdminService`: identity, capability tokens, relations and
//! policies.
//!
//! Admin requests carry no `repo` field; the repository is named inside
//! the object or policy name instead:
//!
//! * relation objects are `repo:<repo>` or `path:<repo>/<dir>`;
//! * capability scopes are `repo:<repo>` or `repo:<repo>/<dir>[/*]`;
//! * policy names are `<repo>/submit` (Cedar) or `<repo>/risk` (JSON).

use std::sync::Arc;

use rill_policy::{Capability, Relation, Subject, Verb};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::admin_service_server::AdminService;
use rill_types::{Principal, RepoPathBuf, Timestamp};
use tonic::{Request, Response, Status};

use super::common::{self, open, GrpcResult, Opened};
use super::idem;
use crate::auth::caller;
use crate::error::{Error, Result};
use crate::state::policy::{self, StoredPolicy};
use crate::state::AppState;

/// The admin service.
#[derive(Clone)]
pub struct AdminSvc {
    state: Arc<AppState>,
}

impl AdminSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        AdminSvc { state }
    }

    async fn open_named<T>(&self, request: &Request<T>, repo: &str) -> Result<Opened> {
        open(&self.state, request, repo).await
    }
}

/// `repo:<repo>` or `path:<repo>/<dir>` to (repo, authorizer object).
fn parse_object(object: &str) -> Result<(String, String)> {
    if let Some(repo) = object.strip_prefix("repo:") {
        if repo.is_empty() || repo.contains('/') {
            return Err(Error::invalid(format!("invalid object {object:?}")));
        }
        return Ok((
            repo.to_string(),
            rill_policy::rebac::REPO_OBJECT.to_string(),
        ));
    }
    if let Some(rest) = object.strip_prefix("path:") {
        let (repo, dir) = rest.split_once('/').unwrap_or((rest, ""));
        let dir = RepoPathBuf::new(dir)?;
        return Ok((repo.to_string(), rill_policy::rebac::dir_object(&dir)));
    }
    Err(Error::invalid(format!(
        "object {object:?} must be repo:<repo> or path:<repo>/<dir>"
    )))
}

/// The inverse of [`parse_object`].
fn format_object(repo: &str, object: &str) -> String {
    if object == rill_policy::rebac::REPO_OBJECT {
        return format!("repo:{repo}");
    }
    match object.strip_prefix("dir:/") {
        Some(dir) => format!("path:{repo}/{dir}"),
        None => format!("{repo}:{object}"),
    }
}

/// `repo:<repo>[/<dir>][/*]` to (repo, path).
fn parse_scope(scope: &str) -> Result<(String, RepoPathBuf)> {
    let rest = scope
        .strip_prefix("repo:")
        .ok_or_else(|| Error::invalid(format!("scope {scope:?} must start with repo:")))?;
    let rest = rest
        .strip_suffix("/*")
        .or_else(|| rest.strip_suffix('*'))
        .unwrap_or(rest);
    let (repo, dir) = rest.split_once('/').unwrap_or((rest, ""));
    if repo.is_empty() {
        return Err(Error::invalid(format!(
            "scope {scope:?} names no repository"
        )));
    }
    Ok((repo.to_string(), RepoPathBuf::new(dir)?))
}

fn parse_action(action: &str) -> Result<Verb> {
    match action {
        "repo.read" | "read" => Ok(Verb::Read),
        "repo.write" | "write" => Ok(Verb::Write),
        "lander.land" | "repo.land" | "land" => Ok(Verb::Land),
        "review.vote" | "repo.review" | "review" => Ok(Verb::Review),
        "repo.admin" | "admin" => Ok(Verb::Admin),
        other => Err(Error::invalid(format!("unknown action {other:?}"))),
    }
}

fn capability_protos(repo_name: &str, cap: &Capability) -> Vec<pb::Capability> {
    let mut out = Vec::new();
    for verb in &cap.verbs {
        for path in &cap.paths {
            let scope = if path.is_root() {
                format!("repo:{repo_name}/*")
            } else {
                format!("repo:{repo_name}/{path}/*")
            };
            out.push(pb::Capability {
                action: format!("repo.{verb}"),
                scope,
                expires_at: Some(timestamp_to_proto(cap.expires_at)),
            });
        }
    }
    out
}

fn parse_subject(subject: &str) -> Result<Subject> {
    let trimmed = subject.strip_suffix("#member").unwrap_or(subject);
    Ok(trimmed.parse::<Subject>()?)
}

fn parse_relation(relation: &str) -> Result<Relation> {
    Ok(relation.parse::<Relation>()?)
}

/// `<repo>/<policy>`.
fn parse_policy_name(name: &str) -> Result<(String, String)> {
    match name.rsplit_once('/') {
        Some((repo, policy)) if !repo.is_empty() => Ok((repo.to_string(), policy.to_string())),
        _ => Err(Error::invalid(format!(
            "policy name {name:?} must be <repo>/{} or <repo>/{}",
            policy::SUBMIT_POLICY,
            policy::RISK_POLICY
        ))),
    }
}

#[tonic::async_trait]
impl AdminService for AdminSvc {
    async fn who_am_i(
        &self,
        request: Request<pb::WhoAmIRequest>,
    ) -> GrpcResult<pb::WhoAmIResponse> {
        let result: crate::error::Result<Response<pb::WhoAmIResponse>> = async {
            let ctx = caller(&request)?;
            let (capabilities, token_expires_at) = match &ctx.capability {
                Some(cap) => {
                    let name = match self.state.stores.get_repo(&cap.repo).await? {
                        Some(config) => config.name,
                        None => cap.repo.to_string(),
                    };
                    (
                        capability_protos(&name, cap),
                        Some(timestamp_to_proto(cap.expires_at)),
                    )
                }
                None => (Vec::new(), None),
            };
            Ok(Response::new(pb::WhoAmIResponse {
                principal: Some(pb::Principal::from(&ctx.principal)),
                teams: ctx.teams.clone(),
                capabilities,
                token_expires_at,
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    /// Mint a capability token. A caller holding a capability token may
    /// only delegate a narrower one (to itself or a sponsored agent); a
    /// caller without one must be an owner of the repository.
    async fn issue_token(
        &self,
        request: Request<pb::IssueTokenRequest>,
    ) -> GrpcResult<pb::IssueTokenResponse> {
        let result: crate::error::Result<Response<pb::IssueTokenResponse>> = idem::once(&self.state, request, |request| async move {
            let ctx = caller(&request)?;
            let req = request.get_ref();
            if req.capabilities.is_empty() {
                return Err(Error::invalid("at least one capability is required"));
            }
            let mut repo_name: Option<String> = None;
            let mut verbs = Vec::new();
            let mut paths = Vec::new();
            for cap in &req.capabilities {
                let (repo, path) = parse_scope(&cap.scope)?;
                match &repo_name {
                    Some(existing) if *existing != repo => {
                        return Err(Error::invalid("all capabilities must target one repository"))
                    }
                    _ => repo_name = Some(repo),
                }
                let verb = parse_action(&cap.action)?;
                if !verbs.contains(&verb) {
                    verbs.push(verb);
                }
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            let repo = self
                .state
                .repos
                .resolve(repo_name.as_deref().unwrap_or_default())
                .await?;
            let opened = common::opened(&self.state, ctx.clone(), repo.clone());
            let principal = match req.for_principal.clone() {
                Some(p) => Principal::try_from(p)?,
                None => ctx.principal.clone(),
            };
            let now = Timestamp::now();
            let longest = self.state.issuer.lifetimes().max_for(principal.kind);
            let expires_at = req
                .expires_at
                .map(Timestamp::from_millis)
                .unwrap_or_else(|| Timestamp::from_millis(now.millis() + longest));
            if expires_at <= Timestamp::now() {
                return Err(Error::invalid("expires_at is in the past"));
            }
            let capability = Capability::repo_wide(repo.id(), verbs, expires_at).with_paths(paths);
            capability.validate()?;
            match &ctx.capability {
                Some(parent) => capability.check_narrower_than(parent)?,
                None => opened.permit().admin().await?,
            }
            let org = ctx
                .claims
                .as_ref()
                .map_or(rill_store::repo::DEFAULT_ORG, |c| c.org.as_str());
            let claims = rill_policy::TokenClaims::new(org, now, expires_at.millis() - now.millis());
            let token = self.state.issuer.issue(&principal, &claims, Some(&capability))?;
            tracing::info!(by = %ctx.principal, for_principal = %principal, repo = %repo.config.name, "token issued");
            Ok(Response::new(pb::IssueTokenResponse {
                token,
                expires_at: timestamp_to_proto(expires_at),
            }))
        })
        .await;
        result.map_err(Status::from)
    }

    async fn add_relation(
        &self,
        request: Request<pb::AddRelationRequest>,
    ) -> GrpcResult<pb::AddRelationResponse> {
        let result: crate::error::Result<Response<pb::AddRelationResponse>> =
            idem::once(&self.state, request, |request| async move {
                let rel = request
                    .get_ref()
                    .relation
                    .clone()
                    .ok_or_else(|| Error::invalid("relation is required"))?;
                let (repo, object) = parse_object(&rel.object)?;
                let opened = self.open_named(&request, &repo).await?;
                opened.permit().admin().await?;
                opened
                    .repo
                    .authorizer
                    .grant(
                        &object,
                        parse_relation(&rel.relation)?,
                        &parse_subject(&rel.subject)?,
                        opened.principal(),
                    )
                    .await?;
                Ok(Response::new(pb::AddRelationResponse {}))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn remove_relation(
        &self,
        request: Request<pb::RemoveRelationRequest>,
    ) -> GrpcResult<pb::RemoveRelationResponse> {
        let result: crate::error::Result<Response<pb::RemoveRelationResponse>> =
            idem::once(&self.state, request, |request| async move {
                let rel = request
                    .get_ref()
                    .relation
                    .clone()
                    .ok_or_else(|| Error::invalid("relation is required"))?;
                let (repo, object) = parse_object(&rel.object)?;
                let opened = self.open_named(&request, &repo).await?;
                opened.permit().admin().await?;
                opened
                    .repo
                    .authorizer
                    .revoke(
                        &object,
                        parse_relation(&rel.relation)?,
                        &parse_subject(&rel.subject)?,
                        opened.principal(),
                    )
                    .await?;
                Ok(Response::new(pb::RemoveRelationResponse {}))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn list_relations(
        &self,
        request: Request<pb::ListRelationsRequest>,
    ) -> GrpcResult<pb::ListRelationsResponse> {
        let result: crate::error::Result<Response<pb::ListRelationsResponse>> = async {
            let req = request.get_ref();
            let (repo, object) = parse_object(&req.object)?;
            let opened = self.open_named(&request, &repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let filter = req.relation.clone().filter(|r| !r.is_empty());
            let relations = opened
                .repo
                .repo
                .relations_of(&object)
                .await?
                .into_iter()
                .filter(|(rel, _)| filter.as_ref().is_none_or(|f| f == rel))
                .map(|(relation, subject)| pb::Relation {
                    object: format_object(&opened.repo.config.name, &object),
                    relation,
                    subject,
                })
                .collect();
            Ok(Response::new(pb::ListRelationsResponse { relations }))
        }
        .await;
        result.map_err(Status::from)
    }

    async fn get_policy(
        &self,
        request: Request<pb::GetPolicyRequest>,
    ) -> GrpcResult<pb::GetPolicyResponse> {
        let result: crate::error::Result<Response<pb::GetPolicyResponse>> = async {
            let (repo, name) = parse_policy_name(&request.get_ref().name)?;
            let opened = self.open_named(&request, &repo).await?;
            opened.permit().read(&RepoPathBuf::root()).await?;
            let stored = match policy::load(&opened.repo.repo, &name).await? {
                Some(stored) => stored,
                None => StoredPolicy {
                    text: policy::default_text(&name, &self.state.config.risk)?,
                    updated_at: opened.repo.config.created_at,
                    updated_by: rill_lander::revert::lander_principal(),
                },
            };
            Ok(Response::new(pb::GetPolicyResponse {
                name: request.get_ref().name.clone(),
                text: stored.text,
                updated_at: timestamp_to_proto(stored.updated_at),
                updated_by: Some(pb::Principal::from(stored.updated_by)),
            }))
        }
        .await;
        result.map_err(Status::from)
    }

    /// Append an imported version to trunk: the one way, besides landing,
    /// for history to reach trunk, so importers can bring existing history
    /// over. Needs the admin permit.
    async fn append_trunk(
        &self,
        request: Request<pb::AppendTrunkRequest>,
    ) -> GrpcResult<pb::AppendTrunkResponse> {
        let result: crate::error::Result<Response<pb::AppendTrunkResponse>> =
            idem::once(&self.state, request, |request| async move {
                let opened = self.open_named(&request, &request.get_ref().repo).await?;
                opened.permit().admin().await?;
                let req = request.get_ref();
                let expected =
                    rill_proto::convert::opt_hash_from_proto("expected", req.expected.as_ref())?;
                let version = rill_proto::convert::hash_from_proto("version", &req.version)?;
                let seq = opened
                    .repo
                    .repo
                    .land_on_trunk(expected, version)
                    .await
                    .map_err(|e| {
                        if e.is_conflict() {
                            Error::FailedPrecondition("trunk is not at the expected head".into())
                        } else {
                            e.into()
                        }
                    })?;
                Ok(Response::new(pb::AppendTrunkResponse { seq: seq.get() }))
            })
            .await;
        result.map_err(Status::from)
    }

    /// Validate, store and hot-swap a policy.
    async fn set_policy(
        &self,
        request: Request<pb::SetPolicyRequest>,
    ) -> GrpcResult<pb::SetPolicyResponse> {
        let result: crate::error::Result<Response<pb::SetPolicyResponse>> =
            idem::once(&self.state, request, |request| async move {
                let (repo, name) = parse_policy_name(&request.get_ref().name)?;
                let opened = self.open_named(&request, &repo).await?;
                opened.permit().admin().await?;
                let text = request.get_ref().text.clone();
                policy::validate(&name, &text)?;
                let stored = StoredPolicy {
                    text,
                    updated_at: Timestamp::now(),
                    updated_by: opened.principal().clone(),
                };
                policy::store(&opened.repo.repo, &name, &stored).await?;
                opened.repo.reload_policy().await?;
                Ok(Response::new(pb::SetPolicyResponse {
                    updated_at: timestamp_to_proto(stored.updated_at),
                }))
            })
            .await;
        result.map_err(Status::from)
    }

    async fn create_org(
        &self,
        request: Request<pb::CreateOrgRequest>,
    ) -> GrpcResult<pb::CreateOrgResponse> {
        Ok(idem::once(&self.state, request, |r| async move {
            Ok(Response::new(super::orgs::create(&self.state, r).await?))
        })
        .await?)
    }

    async fn list_orgs(
        &self,
        request: Request<pb::ListOrgsRequest>,
    ) -> GrpcResult<pb::ListOrgsResponse> {
        Ok(Response::new(
            super::orgs::list(&self.state, request).await?,
        ))
    }

    async fn health(&self, _request: Request<pb::HealthRequest>) -> GrpcResult<pb::HealthResponse> {
        let result: crate::error::Result<Response<pb::HealthResponse>> = async {
            let components = health_components(&self.state).await;
            Ok(Response::new(pb::HealthResponse {
                ok: components.iter().all(|c| c.ok),
                version: env!("CARGO_PKG_VERSION").to_string(),
                components,
                attestation_key: rill_policy::KeyFile::encode_public(
                    &self.state.attestor.public_key(),
                ),
            }))
        }
        .await;
        result.map_err(Status::from)
    }
}

/// Probe each dependency the server needs to answer requests.
pub async fn health_components(state: &AppState) -> Vec<pb::ComponentHealth> {
    let meta = match state.repos.list().await {
        Ok(repos) => pb::ComponentHealth {
            name: "meta_store".into(),
            ok: true,
            detail: Some(format!("{} repositories", repos.len())),
        },
        Err(e) => pb::ComponentHealth {
            name: "meta_store".into(),
            ok: false,
            detail: Some(e.to_string()),
        },
    };
    let probe = rill_types::ObjectHash::digest(rill_types::ObjectKind::Blob, b"");
    let objects = match state
        .stores
        .objects
        .has(rill_types::ObjectKind::Blob, &probe)
        .await
    {
        Ok(_) => pb::ComponentHealth {
            name: "object_store".into(),
            ok: true,
            detail: None,
        },
        Err(e) => pb::ComponentHealth {
            name: "object_store".into(),
            ok: false,
            detail: Some(e.to_string()),
        },
    };
    let uptime = pb::ComponentHealth {
        name: "server".into(),
        ok: true,
        detail: Some(format!("up {}s", state.started_at.elapsed().as_secs())),
    };
    vec![meta, objects, uptime]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_objects_scopes_and_names() {
        assert_eq!(
            parse_object("repo:main").unwrap(),
            ("main".into(), "repo".into())
        );
        assert_eq!(
            parse_object("path:main/src/auth").unwrap(),
            ("main".into(), "dir:/src/auth".into())
        );
        assert!(parse_object("team:x").is_err());
        assert_eq!(format_object("main", "dir:/src"), "path:main/src");
        assert_eq!(format_object("main", "repo"), "repo:main");
        assert_eq!(
            parse_scope("repo:main/src/*").unwrap(),
            ("main".into(), RepoPathBuf::new("src").unwrap())
        );
        assert_eq!(parse_scope("repo:main").unwrap().1, RepoPathBuf::root());
        assert_eq!(parse_action("repo.write").unwrap(), Verb::Write);
        assert_eq!(
            parse_policy_name("main/submit").unwrap(),
            ("main".into(), "submit".into())
        );
        assert!(parse_policy_name("submit").is_err());
        assert!(
            matches!(parse_subject("team:infra#member").unwrap(), Subject::Team(t) if t == "infra")
        );
    }
}
