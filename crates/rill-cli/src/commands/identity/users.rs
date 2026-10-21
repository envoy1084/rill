//! `rill admin user`: an organization's users.

use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use serde_json::json;

use super::providers::OrgArg;
use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill admin user <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum UserCmd {
    /// The org's users.
    List(OrgArg),
    /// Suspend a user: every token they hold stops working.
    Suspend(UserArg),
    /// Reactivate a suspended user.
    Activate(UserArg),
    /// Set a user's roles (`admin`, or none).
    Roles(RolesArgs),
}

/// A user of an org.
#[derive(Args, Debug)]
pub struct UserArg {
    /// The user's name.
    pub name: String,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

/// `rill admin user roles`.
#[derive(Args, Debug)]
pub struct RolesArgs {
    /// The user's name.
    pub name: String,
    /// Roles, comma-separated; empty to clear.
    #[arg(long, value_delimiter = ',')]
    pub roles: Vec<String>,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

fn user_json(u: &pb::User) -> serde_json::Value {
    json!({
        "org": u.org, "name": u.principal.as_ref().map(|p| p.name.clone()),
        "email": u.email, "status": u.status, "roles": u.roles, "teams": u.teams,
        "external": u.external,
    })
}

async fn update(
    ctx: &mut Ctx<'_>,
    org: String,
    name: String,
    suspended: Option<bool>,
    roles: Option<Vec<String>>,
) -> Result<()> {
    let user = ctx
        .client()?
        .identity_svc()
        .update_user(pb::UpdateUserRequest {
            operation_id: Default::default(),
            org,
            name,
            suspended,
            set_roles: roles.is_some(),
            roles: roles.unwrap_or_default(),
        })
        .await?
        .into_inner()
        .user
        .unwrap_or_default();
    ctx.emit(user_json(&user), |out| {
        out.line(format!(
            "{} is {} ({})",
            user.principal
                .as_ref()
                .map(|p| p.name.as_str())
                .unwrap_or_default(),
            user.status,
            if user.roles.is_empty() {
                "no roles".into()
            } else {
                user.roles.join(", ")
            }
        ))
    })
}

/// Run `rill admin user`.
pub async fn run(ctx: &mut Ctx<'_>, cmd: UserCmd) -> Result<()> {
    match cmd {
        UserCmd::List(o) => {
            let users = ctx
                .client()?
                .identity_svc()
                .list_users(pb::ListUsersRequest { org: o.org })
                .await?
                .into_inner()
                .users;
            ctx.emit(
                json!(users.iter().map(user_json).collect::<Vec<_>>()),
                |out| {
                    let mut table = Table::new(&["NAME", "STATUS", "ROLES", "TEAMS", "EMAIL"]);
                    for u in &users {
                        table.row(vec![
                            u.principal
                                .as_ref()
                                .map(|p| p.name.clone())
                                .unwrap_or_default(),
                            u.status.clone(),
                            u.roles.join(","),
                            u.teams.join(","),
                            u.email.clone().unwrap_or_default(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        UserCmd::Suspend(a) => update(ctx, a.org.org, a.name, Some(true), None).await,
        UserCmd::Activate(a) => update(ctx, a.org.org, a.name, Some(false), None).await,
        UserCmd::Roles(a) => update(ctx, a.org.org, a.name, None, Some(a.roles)).await,
    }
}
