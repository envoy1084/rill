//! `rill admin scim`: credentials for an org's provisioning system.

use clap::Subcommand;
use rill_proto::rill::v1 as pb;
use serde_json::json;

use super::agents::AgentArg;
use super::providers::OrgArg;
use crate::context::Ctx;
use crate::output::{when, Table};
use crate::Result;

/// `rill admin scim <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ScimCmd {
    /// Create a credential; its token is shown once.
    Create(OrgArg),
    /// The org's credentials.
    List(OrgArg),
    /// Revoke a credential by id.
    Revoke(AgentArg),
}

/// Run `rill admin scim`.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ScimCmd) -> Result<()> {
    let svc = ctx.client()?.identity_svc();
    match cmd {
        ScimCmd::Create(o) => {
            let created = svc
                .create_scim_credential(pb::CreateScimCredentialRequest {
                    request_id: String::new(),
                    parent: rill_sdk::names::org(&o.org),
                })
                .await?
                .into_owned();
            let id = created.credential.map(|c| c.id).unwrap_or_default();
            ctx.emit(
                json!({ "id": id, "token": created.token, "org": o.org }),
                |out| {
                    out.line(format!("credential {id} for /scim/v2/{}", o.org))?;
                    out.line(format!("token (shown once): {}", created.token))
                },
            )
        }
        ScimCmd::List(o) => {
            let credentials = svc
                .list_scim_credentials(pb::ListScimCredentialsRequest {
                    parent: rill_sdk::names::org(&o.org),
                    page_size: 1000,
                    page_token: String::new(),
                })
                .await?
                .into_owned()
                .credentials;
            let value = json!(credentials
                .iter()
                .map(|c| json!({ "id": c.id, "created_at": c.created_at, "revoked_at": c.revoked_at }))
                .collect::<Vec<_>>());
            ctx.emit(value, |out| {
                let mut table = Table::new(&["ID", "CREATED", "REVOKED"]);
                for c in &credentials {
                    let ts = |ms: i64| when(&rill_types::Timestamp::from_millis(ms));
                    table.row(vec![
                        c.id.clone(),
                        ts(c.created_at),
                        c.revoked_at.map(ts).unwrap_or_default(),
                    ]);
                }
                out.table(table)
            })
        }
        ScimCmd::Revoke(a) => {
            svc.delete_scim_credential(pb::DeleteScimCredentialRequest {
                request_id: String::new(),
                name: format!(
                    "{}/scimCredentials/{}",
                    rill_sdk::names::org(&a.org.org),
                    a.name
                ),
            })
            .await?;
            ctx.emit(json!({ "revoked": a.name }), |out| {
                out.line(format!("revoked {}", a.name))
            })
        }
    }
}
