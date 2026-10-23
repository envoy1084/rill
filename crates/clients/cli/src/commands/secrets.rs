//! `rill admin secret`: org secrets verify plans give their steps.

use std::io::Read;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use serde_json::json as j;

use super::audit::OrgArg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{when, Table};
use crate::Result;

/// `rill admin secret <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum SecretCmd {
    /// Set a secret from standard input (or a file); values never go on
    /// the command line.
    Set(SetArgs),
    /// Secret names and where they may be used; never values.
    List(OrgArg),
    /// Delete a secret.
    Delete(DeleteArgs),
}

/// `rill admin secret set`.
#[derive(Args, Debug)]
pub struct SetArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// The name steps list in `secrets`, an environment variable name.
    pub name: String,
    /// Read the value from this file instead of standard input.
    #[arg(long)]
    pub from_file: Option<PathBuf>,
    /// Only these repositories' plans may use it (repeatable); default
    /// every repository.
    #[arg(long = "allow-repo")]
    pub repos: Vec<String>,
    /// Only verification after landing gets it.
    #[arg(long)]
    pub trunk_only: bool,
}

/// `rill admin secret delete`.
#[derive(Args, Debug)]
pub struct DeleteArgs {
    /// The org.
    #[command(flatten)]
    pub org: OrgArg,
    /// The secret.
    pub name: String,
}

/// A secret's own name, the last segment of its resource name.
fn short(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn secret_json(s: &pb::Secret) -> serde_json::Value {
    j!({
        "name": short(&s.name),
        "repos": s.repos,
        "trunk_only": s.trunk_only,
        "updated_at": s.updated_at,
        "updated_by": s.updated_by.as_option().map(|p| p.name.clone()),
    })
}

/// The value: the file's bytes, or standard input without the trailing
/// newline a terminal or `echo` adds.
fn read_value(from_file: Option<PathBuf>) -> Result<Vec<u8>> {
    let mut value = match from_file {
        Some(path) => std::fs::read(&path)
            .map_err(|e| CliError::usage(format!("reading {}: {e}", path.display())))?,
        None => {
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| CliError::usage(format!("reading standard input: {e}")))?;
            buf
        }
    };
    while matches!(value.last(), Some(b'\n' | b'\r')) {
        value.pop();
    }
    Ok(value)
}

/// Run a secret subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: SecretCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        SecretCmd::Set(a) => {
            let value = read_value(a.from_file)?;
            let secret = client
                .secrets(&a.org.org)
                .set(&a.name, &value, a.repos, a.trunk_only)
                .await?;
            ctx.emit(secret_json(&secret), |out| {
                out.line(format!("secret {} set", short(&secret.name)))
            })
        }
        SecretCmd::List(o) => {
            let list = client.secrets(&o.org).list().await?;
            ctx.emit(
                j!({ "secrets": list.iter().map(secret_json).collect::<Vec<_>>() }),
                |out| {
                    let mut table = Table::new(&["NAME", "REPOS", "TRUNK ONLY", "UPDATED", "BY"]);
                    for s in &list {
                        table.row(vec![
                            short(&s.name).to_string(),
                            if s.repos.is_empty() {
                                "all".into()
                            } else {
                                s.repos.join(",")
                            },
                            s.trunk_only.to_string(),
                            when(&Timestamp::from_millis(s.updated_at)),
                            s.updated_by
                                .as_option()
                                .map(|p| p.name.clone())
                                .unwrap_or_default(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        SecretCmd::Delete(a) => {
            client.secrets(&a.org.org).delete(&a.name).await?;
            ctx.emit(j!({ "deleted": true }), |out| {
                out.line(format!("secret {} deleted", a.name))
            })
        }
    }
}
