//! An organization's users.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::{AccountStatus, OrgRole, UserRecord};
use rill_types::Timestamp;

use crate::auth::{caller, require_org_admin};
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::{etag, names, Call};
use rill_proto::names::USER;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// The wire form of a user, with its name and etag.
pub fn user_proto(u: &UserRecord) -> pb::User {
    let mut out = user_fields(u);
    out.etag = etag::of(&out);
    out
}

fn user_fields(u: &UserRecord) -> pb::User {
    pb::User {
        name: names::org_resource(&USER, &u.org, &u.principal.name),
        etag: String::new(),
        org: u.org.clone(),
        principal: pb::Principal::from(&u.principal).into(),
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
    request: Call<pb::ListUsersRequest>,
) -> Result<pb::ListUsersResponse> {
    let ctx = caller(&request)?;
    let req = request.get_ref();
    let org = names::org(&req.parent)?;
    require_org_admin(state, &ctx, &org).await?;
    let filter = Filter::parse(
        &req.filter,
        &[
            ("status", Kind::Str),
            ("roles", Kind::List),
            ("teams", Kind::List),
        ],
    )?;
    let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
    let users: Vec<pb::User> = state
        .identity
        .store()
        .users(&org)
        .await?
        .iter()
        .map(user_proto)
        .filter(|u| filter.matches(&Listed(u)))
        .collect();
    let (users, next_page_token) = page.of(users)?;
    Ok(pb::ListUsersResponse {
        users,
        next_page_token,
    })
}

/// Suspend or reactivate a user, or set their roles. Suspension refuses
/// every token the user holds at once.
pub async fn update(
    state: &AppState,
    request: Call<pb::UpdateUserRequest>,
) -> Result<pb::UpdateUserResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    let given = req
        .user
        .into_option()
        .ok_or(rill_proto::ConvertError::missing("user"))?;
    let (org, name) = names::org_child(&USER, &given.name)?;
    require_org_admin(state, &ctx, &org).await?;
    let mask = req.update_mask.into_option().unwrap_or_default().paths;
    if mask.is_empty() {
        return Err(rill_proto::ConvertError::missing("update_mask").into());
    }
    let store = state.identity.store();
    let before = store
        .user(&org, &name)
        .await?
        .ok_or_else(|| Error::NotFound(format!("user {org}/{name}")))?;
    etag::check(&user_proto(&before).etag, &req.etag, "the user")?;
    let mut after = before.clone();
    for path in &mask {
        match path.as_str() {
            "status" => {
                after.status = match given.status.as_str() {
                    "active" => AccountStatus::Active,
                    "suspended" => AccountStatus::Suspended,
                    other => {
                        return Err(Error::invalid(format!(
                            "status is active or suspended, not {other:?}"
                        )))
                    }
                }
            }
            "roles" => {
                let mut roles = given
                    .roles
                    .iter()
                    .map(|r| parse_role(r))
                    .collect::<Result<Vec<_>>>()?;
                roles.sort();
                roles.dedup();
                after.roles = roles;
            }
            other => {
                return Err(Error::invalid(format!(
                    "update_mask names {other:?}; it may name status and roles"
                )))
            }
        }
    }
    after.updated_at = Timestamp::now();
    store.update_user(&before, &after, &ctx.principal).await?;
    tracing::info!(by = %ctx.principal, user = %after.principal, org = %after.org, status = ?after.status, "user updated");
    Ok(pb::UpdateUserResponse {
        user: user_proto(&after).into(),
    })
}

/// A user as a list filter sees it.
struct Listed<'a>(&'a pb::User);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "status" => FieldValue::Str(self.0.status.clone()),
            "roles" => FieldValue::List(self.0.roles.clone()),
            "teams" => FieldValue::List(self.0.teams.clone()),
            _ => FieldValue::Missing,
        }
    }
}
