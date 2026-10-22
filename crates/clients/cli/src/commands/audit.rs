//! `rill admin audit`: an org's audit log, its seals and their
//! verification, and export for a SIEM.

use std::io::Write;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use futures::StreamExt;
use rill_types::audit::{AuditEntry, AuditOutcome};
use serde_json::json as j;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{when, Table};
use crate::Result;

/// `rill admin audit <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum AuditCmd {
    /// Entries of the chain.
    List(ListArgs),
    /// Print entries as they are chained.
    Watch(WatchArgs),
    /// Seal what the chain gained since its last seal.
    Seal(OrgArg),
    /// Check every link and every seal of the chain.
    Verify(OrgArg),
    /// Write the chain for a SIEM.
    Export(ExportArgs),
}

/// `--org`, defaulting to the default org; `_instance` for instance-wide
/// actions.
#[derive(Args, Debug)]
pub struct OrgArg {
    /// The org.
    #[arg(long, default_value = "default")]
    pub org: String,
}

/// `rill admin audit list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// Entries after this one.
    #[arg(long, default_value_t = 0)]
    pub after: u64,
    /// Most entries to show.
    #[arg(long, default_value_t = 100)]
    pub limit: u32,
}

/// `rill admin audit watch`.
#[derive(Args, Debug)]
pub struct WatchArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// Start after this entry (0: the whole chain).
    #[arg(long, default_value_t = 0)]
    pub after: u64,
}

/// Export formats.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Format {
    /// One Rill entry per line.
    Json,
    /// OCSF API Activity events (class 6003), one per line.
    Ocsf,
}

/// `rill admin audit export`.
#[derive(Args, Debug)]
pub struct ExportArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// What to write.
    #[arg(long, value_enum, default_value = "ocsf")]
    pub format: Format,
    /// File to write (default: stdout).
    #[arg(long)]
    pub out: Option<PathBuf>,
}

fn entry_json(e: &AuditEntry) -> serde_json::Value {
    let r = &e.record;
    j!({
        "seq": e.seq,
        "hash": hex::encode(e.hash()),
        "prev": hex::encode(e.prev),
        "org": r.org,
        "at": r.at.millis(),
        "actor": r.actor.to_string(),
        "action": r.action,
        "target": r.target,
        "allowed": matches!(r.outcome, AuditOutcome::Allowed),
        "denied_reason": match &r.outcome { AuditOutcome::Denied { reason } => Some(reason), _ => None },
        "detail": r.detail,
    })
}

/// An entry as an OCSF 1.x API Activity event.
pub fn ocsf(e: &AuditEntry) -> serde_json::Value {
    let r = &e.record;
    let (status_id, status, reason) = match &r.outcome {
        AuditOutcome::Allowed => (1, "Success", None),
        AuditOutcome::Denied { reason } => (2, "Failure", Some(reason.clone())),
    };
    j!({
        "class_uid": 6003,
        "class_name": "API Activity",
        "category_uid": 6,
        "activity_id": 99,
        "activity_name": r.action,
        "time": r.at.millis(),
        "status_id": status_id,
        "status": status,
        "status_detail": reason,
        "severity_id": 1,
        "actor": { "user": { "name": r.actor.name, "type": r.actor.kind.as_str() } },
        "api": { "operation": r.action, "service": { "name": "rill" } },
        "resources": [{ "name": r.target }],
        "metadata": {
            "product": { "name": "Rill", "vendor_name": "Rill" },
            "version": "1.1.0",
            "sequence": e.seq,
            "uid": hex::encode(e.hash()),
            "tenant_uid": r.org,
        },
        "unmapped": r.detail,
    })
}

/// Run an audit subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: AuditCmd) -> Result<()> {
    let audit = ctx.client()?.audit();
    match cmd {
        AuditCmd::List(a) => {
            let (entries, head) = audit.entries(&a.org.org, a.after, a.limit).await?;
            ctx.emit(
                j!({ "entries": entries.iter().map(entry_json).collect::<Vec<_>>(), "head": head }),
                |out| {
                    let mut table = Table::new(&["SEQ", "WHEN", "ACTOR", "ACTION", "TARGET"]);
                    for e in &entries {
                        let r = &e.record;
                        let action = match &r.outcome {
                            AuditOutcome::Allowed => r.action.clone(),
                            AuditOutcome::Denied { .. } => format!("{} (denied)", r.action),
                        };
                        table.row(vec![
                            e.seq.to_string(),
                            when(&r.at),
                            r.actor.to_string(),
                            action,
                            r.target.clone(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        AuditCmd::Watch(a) => {
            let mut stream = audit.watch(&a.org.org, a.after).await?;
            while let Some(entry) = stream.next().await {
                let entry = entry?;
                ctx.emit(entry_json(&entry), |out| {
                    out.line(format!(
                        "{} {} {} {}",
                        entry.seq, entry.record.actor, entry.record.action, entry.record.target
                    ))
                })?;
            }
            Ok(())
        }
        AuditCmd::Seal(o) => {
            let root = audit.seal(&o.org).await?;
            ctx.emit(
                j!({ "sealed": root.as_ref().map(|r| j!({ "first_seq": r.first_seq, "last_seq": r.last_seq, "export": r.export.to_string() })) }),
                |out| match &root {
                    Some(r) => out.line(format!("sealed entries {}..={}", r.first_seq, r.last_seq)),
                    None => out.line(out.dim("nothing new to seal")),
                },
            )
        }
        AuditCmd::Verify(o) => {
            let entries = audit.all_entries(&o.org).await?;
            let roots = audit.roots(&o.org).await?;
            let keys = audit.keys().await?;
            match rill_policy::audit::verify_chain(&entries, &roots, &keys) {
                Ok(report) => ctx.emit(
                    j!({ "ok": true, "entries": report.entries, "seals": report.roots, "sealed_through": report.sealed_through }),
                    |out| {
                        out.line(format!(
                            "{} entries, {} seals: the chain is intact (sealed through {})",
                            report.entries, report.roots, report.sealed_through
                        ))
                    },
                ),
                Err(broken) => Err(CliError::conflict(broken.to_string())),
            }
        }
        AuditCmd::Export(a) => {
            let entries = audit.all_entries(&a.org.org).await?;
            let mut writer: Box<dyn Write> = match &a.out {
                Some(path) => Box::new(std::fs::File::create(path)?),
                None => Box::new(std::io::stdout()),
            };
            for e in &entries {
                let line = match a.format {
                    Format::Json => entry_json(e),
                    Format::Ocsf => ocsf(e),
                };
                serde_json::to_writer(&mut writer, &line)?;
                writer.write_all(b"\n")?;
            }
            writer.flush()?;
            Ok(())
        }
    }
}
