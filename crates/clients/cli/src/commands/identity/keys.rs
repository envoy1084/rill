//! `rill admin key` and `rill admin revoke`.

use clap::Subcommand;
use rill_proto::rill::v1 as pb;
use serde_json::json;

use super::RevokeCmd;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{when, Table};
use crate::Result;

/// `rill admin key <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum KeyCmd {
    /// The token signing keys and their lifetimes.
    List,
    /// Add a signing key now; it signs after the activation delay.
    Rotate,
}

fn ts(ms: i64) -> String {
    when(&rill_types::Timestamp::from_millis(ms))
}

/// Run `rill admin key`.
pub async fn run(ctx: &mut Ctx<'_>, cmd: KeyCmd) -> Result<()> {
    let svc = ctx.client()?.identity_svc();
    match cmd {
        KeyCmd::List => {
            let keys = svc
                .list_token_keys(pb::ListTokenKeysRequest {})
                .await?
                .into_owned()
                .keys;
            let value = json!(keys
                .iter()
                .map(|k| json!({
                    "id": k.id, "public_key": k.public_key, "backend": k.backend,
                    "key_ref": k.key_ref, "created_at": k.created_at,
                    "active_from": k.active_from, "not_after": k.not_after,
                }))
                .collect::<Vec<_>>());
            ctx.emit(value, |out| {
                let mut table = Table::new(&["ID", "BACKEND", "ACTIVE FROM", "RETIRES"]);
                for k in &keys {
                    table.row(vec![
                        k.id.to_string(),
                        format!("{}:{}", k.backend, k.key_ref),
                        ts(k.active_from),
                        k.not_after.map(ts).unwrap_or_default(),
                    ]);
                }
                out.table(table)
            })
        }
        KeyCmd::Rotate => {
            let rotated = svc
                .rotate_token_keys(pb::RotateTokenKeysRequest {
                    operation_id: Default::default(),
                })
                .await?
                .into_owned();
            ctx.emit(
                json!({ "key_id": rotated.key_id, "active_from": rotated.active_from }),
                |out| match rotated.key_id {
                    Some(id) => {
                        out.line(format!("key {id} signs from {}", ts(rotated.active_from)))
                    }
                    None => out.line("another server rotated at the same moment"),
                },
            )
        }
    }
}

/// Run `rill admin revoke`.
pub async fn revoke(ctx: &mut Ctx<'_>, cmd: RevokeCmd) -> Result<()> {
    let svc = ctx.client()?.identity_svc();
    match cmd {
        RevokeCmd::Token(args) => {
            let target = match args.token_id {
                Some(id) => pb::revoke_token_request::Target::TokenId(id),
                None => {
                    let token = ctx.client_config()?.token.ok_or_else(|| {
                        CliError::usage("name a token id, or pass the token with --token")
                    })?;
                    pb::revoke_token_request::Target::Token(token)
                }
            };
            let revoked = svc
                .revoke_token(pb::RevokeTokenRequest {
                    operation_id: Default::default(),
                    target: Some(target),
                    reason: args.reason,
                })
                .await?
                .into_owned();
            ctx.emit(json!({ "revoked": revoked.token_id }), |out| {
                out.line(format!("revoked {}", revoked.token_id))
            })
        }
        RevokeCmd::Session(args) => {
            svc.revoke_session(pb::RevokeSessionRequest {
                operation_id: Default::default(),
                session: args.session.clone(),
            })
            .await?;
            ctx.emit(json!({ "ended": args.session }), |out| {
                out.line(format!("ended {}", args.session))
            })
        }
    }
}
