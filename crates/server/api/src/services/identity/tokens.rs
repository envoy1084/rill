//! Describing the caller and minting capability tokens.

use rill_policy::Capability;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{Principal, Timestamp};

use super::scopes::{capability_protos, parse_action, parse_scope};
use super::IdentitySvc;
use crate::auth::caller;
use crate::services::common;
use crate::services::Call;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

impl IdentitySvc {
    pub(super) async fn get_caller_impl(
        &self,
        request: Call<pb::GetCallerRequest>,
    ) -> Result<pb::GetCallerResponse> {
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
        let claims = ctx.claims.as_ref();
        let token_expires_at =
            token_expires_at.or_else(|| claims.map(|c| timestamp_to_proto(c.expires_at)));
        Ok(pb::GetCallerResponse {
            principal: pb::Principal::from(&ctx.principal).into(),
            teams: ctx.teams.clone(),
            capabilities,
            token_expires_at,
            token_id: claims.map(|c| c.token_id.to_string()),
            session: claims.and_then(|c| c.session).map(|s| s.to_string()),
            org: claims.map(|c| c.org.clone()),
            sponsor: claims.and_then(|c| c.sponsor.clone()),
            model: claims.and_then(|c| c.model.clone()),
        })
    }

    pub(super) async fn issue_token_impl(
        &self,
        request: Call<pb::IssueTokenRequest>,
    ) -> Result<pb::IssueTokenResponse> {
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
                    return Err(Error::invalid(
                        "all capabilities must target one repository",
                    ))
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
        let principal = match req.for_principal.clone().into_option() {
            Some(p) => Principal::try_from(p)?,
            None => ctx.principal.clone(),
        };
        if principal.id != ctx.principal.id {
            may_issue_for(&self.state, &ctx, &principal).await?;
        }
        let now = Timestamp::now();
        let issuer = self.state.identity.issuer()?;
        let longest = issuer.lifetimes().max_for(principal.kind);
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
        let token = issuer.issue(&principal, &claims, Some(&capability))?;
        self.state
            .identity
            .store()
            .record(
                rill_types::identity::IdentityChange::TokenIssued {
                    org: org.to_string(),
                    principal: principal.clone(),
                    token_id: claims.token_id,
                    expires_at,
                    scope: Some(format!(
                        "{} {}",
                        repo.config.name,
                        capability
                            .paths
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    )),
                },
                &ctx.principal,
            )
            .await?;
        tracing::info!(by = %ctx.principal, for_principal = %principal, repo = %repo.config.name, "token issued");
        Ok(pb::IssueTokenResponse {
            token,
            expires_at: timestamp_to_proto(expires_at),
        })
    }
}

/// A caller may issue tokens for someone else only for a registered agent
/// it sponsors (itself, or through one of its teams), unless it
/// administers the instance.
async fn may_issue_for(
    state: &AppState,
    ctx: &rill_policy::AuthContext,
    principal: &Principal,
) -> rill_runtime::error::Result<()> {
    if crate::auth::require_instance_admin(state, ctx).is_ok() {
        return Ok(());
    }
    let org = ctx
        .claims
        .as_ref()
        .map_or(rill_store::DEFAULT_ORG, |c| c.org.as_str());
    let sponsored = match state.identity.agent(org, &principal.name).await? {
        Some(agent) if agent.principal == *principal => match &agent.sponsor {
            rill_types::identity::Sponsor::User(name) => *name == ctx.principal.name,
            rill_types::identity::Sponsor::Team(team) => ctx.teams.contains(team),
        },
        _ => false,
    };
    if sponsored {
        Ok(())
    } else {
        Err(Error::denied(format!(
            "{} may issue tokens only for itself and agents it sponsors, not {principal}",
            ctx.principal
        )))
    }
}
