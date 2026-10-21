//! `rill admin protect`: redacting objects, releasing versions secret
//! scanning quarantined, and rotating data keys.

use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use rill_types::{ObjectHash, ObjectKind, Timestamp};
use serde_json::json as j;

use super::audit::OrgArg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{when, Table};
use crate::Result;

/// `rill admin protect <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ProtectCmd {
    /// Ask for an object to be redacted; another administrator approves.
    Redact(RedactArgs),
    /// Approve another administrator's redaction request, applying it.
    Approve(ApproveArgs),
    /// Redaction requests and applied redactions.
    Redactions(OrgArg),
    /// Release a quarantined version so it may land.
    Release(ReleaseArgs),
    /// Add a data key to an org, or wrap every data key again.
    RotateKey(RotateKeyArgs),
}

/// `rill admin protect rotate-key`.
#[derive(Args, Debug)]
pub struct RotateKeyArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// Wrap every org's data keys again under the key service's current
    /// key, instead of adding a key.
    #[arg(long)]
    pub rewrap: bool,
}

/// `rill admin protect redact`.
#[derive(Args, Debug)]
pub struct RedactArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// Object kind (blob, tree, …).
    #[arg(long, default_value = "blob")]
    pub kind: String,
    /// Object hash, hex.
    pub hash: String,
    /// Why, kept in the audit log.
    #[arg(long)]
    pub reason: String,
}

/// `rill admin protect approve`.
#[derive(Args, Debug)]
pub struct ApproveArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// The request id.
    pub id: String,
}

/// `rill admin protect release`.
#[derive(Args, Debug)]
pub struct ReleaseArgs {
    /// The repository.
    #[arg(long)]
    pub repo: String,
    /// The quarantined version, hex.
    #[arg(value_name = "VERSION")]
    pub held: String,
}

fn hash(text: &str) -> Result<ObjectHash> {
    ObjectHash::from_hex(text).map_err(|_| CliError::usage(format!("invalid hash {text:?}")))
}

fn record_json(r: &pb::RedactionRecord) -> serde_json::Value {
    j!({
        "id": r.id,
        "org": r.org,
        "hash": hex::encode(&r.hash),
        "reason": r.reason,
        "requested_by": r.requested_by.as_ref().map(|p| p.name.clone()),
        "approved_by": r.approved_by.as_ref().map(|p| p.name.clone()),
        "applied_at": r.applied_at,
    })
}

fn show(ctx: &mut Ctx<'_>, record: Option<pb::RedactionRecord>) -> Result<()> {
    let record = record.ok_or_else(|| CliError::conflict("the server returned no record"))?;
    ctx.emit(record_json(&record), |out| {
        let state = if record.applied_at.is_some() {
            "applied"
        } else {
            "awaiting a second administrator"
        };
        out.line(format!(
            "redaction {} {}: {state}",
            record.id,
            hex::encode(&record.hash)
        ))
    })
}

/// Run a protection subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ProtectCmd) -> Result<()> {
    let mut svc = ctx.client()?.protection_svc();
    match cmd {
        ProtectCmd::Redact(a) => {
            let kind: ObjectKind = a
                .kind
                .parse()
                .map_err(|_| CliError::usage(format!("unknown object kind {:?}", a.kind)))?;
            let response = svc
                .request_redaction(pb::RequestRedactionRequest {
                    operation_id: Default::default(),
                    org: a.org.org,
                    kind: kind as i32,
                    hash: hash(&a.hash)?.as_bytes().to_vec().into(),
                    reason: a.reason,
                })
                .await?
                .into_inner();
            show(ctx, response.redaction)
        }
        ProtectCmd::Approve(a) => {
            let response = svc
                .approve_redaction(pb::ApproveRedactionRequest {
                    operation_id: Default::default(),
                    org: a.org.org,
                    id: a.id,
                })
                .await?
                .into_inner();
            show(ctx, response.redaction)
        }
        ProtectCmd::Redactions(o) => {
            let list = svc
                .list_redactions(pb::ListRedactionsRequest { org: o.org })
                .await?
                .into_inner()
                .redactions;
            ctx.emit(
                j!({ "redactions": list.iter().map(record_json).collect::<Vec<_>>() }),
                |out| {
                    let mut table =
                        Table::new(&["ID", "HASH", "REQUESTED BY", "APPLIED", "REASON"]);
                    for r in &list {
                        table.row(vec![
                            r.id.clone(),
                            hex::encode(&r.hash),
                            r.requested_by
                                .as_ref()
                                .map(|p| p.name.clone())
                                .unwrap_or_default(),
                            r.applied_at
                                .map(|t| when(&Timestamp::from_millis(t)))
                                .unwrap_or_else(|| "pending".into()),
                            r.reason.clone(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        ProtectCmd::Release(a) => {
            let version = hash(&a.held)?;
            svc.clear_quarantine(pb::ClearQuarantineRequest {
                operation_id: Default::default(),
                repo: a.repo,
                version: version.as_bytes().to_vec().into(),
            })
            .await?;
            ctx.emit(j!({ "released": version.to_hex() }), |out| {
                out.line(format!("released {version}"))
            })
        }
        ProtectCmd::RotateKey(a) => {
            let response = svc
                .rotate_data_keys(pb::RotateDataKeysRequest {
                    operation_id: Default::default(),
                    org: a.org.org,
                    rewrap: a.rewrap,
                })
                .await?
                .into_inner();
            ctx.emit(
                j!({ "key_id": response.key_id, "active_from": response.active_from, "rewrapped": response.rewrapped }),
                |out| match (a.rewrap, response.key_id) {
                    (true, _) => out.line(format!("rewrapped {} data keys", response.rewrapped)),
                    (false, Some(id)) => out.line(format!(
                        "data key {id} added; it seals from {}",
                        when(&Timestamp::from_millis(response.active_from))
                    )),
                    (false, None) => out.line(out.dim("another server added a key at the same moment")),
                },
            )
        }
    }
}
