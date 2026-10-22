//! An organization's users.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::{AccountStatus, OrgRole, UserRecord};
use rill_types::Timestamp;
use tonic::Request;

use crate::auth::{caller, require_org_admin};
use crate::error::{Error, Result};
use crate::state::AppState;

pub fn user_proto(u: &UserRecord) -> pb::User {
    pb::User {
        org: u.org.clone(),
        principal: Some(pb::Principal::from(&u.principal)),
        email: u.email.clone(),
        display_name: u.display_name.clone(),
        status: match u.status {
            AccountStatus::Active => "active",
            AccountStatus::Suspended => "suspended",
        }
        .into(),
        roles: u
            .roles
            .iter()
            .map(|r| match r {
                OrgRole::Admin => "admin".to_string(),
            })
            .collect(),
        teams: u.teams.clone(),
        external: u
            .external
            .iter()
            .map(|e| format!("{}#{}", e.issuer, e.subject))
            .collect(),
        created_at: timestamp_to_proto(u.created_at),
        updated_at: timestamp_to_proto(u.updated_at),
    }
}

fn parse_role(name: &str) -> Result<OrgRole> {
    match name {
        "admin" => Ok(OrgRole::Admin),
        other => Err(Error::invalid(format!("unknown role {other:?}"))),
    }
}

pub async fn list(
    state: &AppState,
    request: Request<pb::ListUsersRequest>,
) -> Result<pb::ListUsersResponse> {
    let ctx = caller(&request)?;
    let org = &request.get_ref().org;
    require_org_admin(state, &ctx, org).await?;
    let users = state.identity.store().users(org).await?;
    Ok(pb::ListUsersResponse {
        users: users.iter().map(user_proto).collect(),
    })
}

pub async fn update(
    state: &AppState,
    request: Request<pb::UpdateUserRequest>,
) -> Result<pb::UpdateUserResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    require_org_admin(state, &ctx, &req.org).await?;
    let store = state.identity.store();
    let before = store
        .user(&req.org, &req.name)
        .await?
        .ok_or_else(|| Error::NotFound(format!("user {}/{}", req.org, req.name)))?;
    let mut after = before.clone();
    if let Some(suspended) = req.suspended {
        after.status = if suspended {
            AccountStatus::Suspended
        } else {
            AccountStatus::Active
        };
    }
    if req.set_roles {
        let mut roles = req
            .roles
            .iter()
            .map(|r| parse_role(r))
            .collect::<Result<Vec<_>>>()?;
        roles.sort();
        roles.dedup();
        after.roles = roles;
    }
    after.updated_at = Timestamp::now();
    store.update_user(&before, &after, &ctx.principal).await?;
    tracing::info!(by = %ctx.principal, user = %after.principal, org = %after.org, status = ?after.status, "user updated");
    Ok(pb::UpdateUserResponse {
        user: Some(user_proto(&after)),
    })
}
