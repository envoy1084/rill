//! `rill admin`: whoami, token issue, relation add/remove/list, policy
//! get/set, health.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_sdk::{Capability, Relation, TokenRequest};
use rill_types::Timestamp;
use serde_json::json as j;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{when, Table};
use crate::Result;

/// `rill admin <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum AdminCmd {
    /// Who the server thinks you are.
    Whoami,
    /// Capability tokens.
    #[command(subcommand)]
    Token(TokenCmd),
    /// ReBAC relationship tuples.
    #[command(subcommand)]
    Relation(RelationCmd),
    /// Named Cedar policies.
    #[command(subcommand)]
    Policy(PolicyCmd),
    /// Server health.
    Health,
}

/// `rill admin token <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum TokenCmd {
    /// Mint an attenuated token.
    Issue(IssueArgs),
}

/// `rill admin token issue`.
#[derive(Args, Debug)]
pub struct IssueArgs {
    /// Repository the token is scoped to (default: the current one).
    #[arg(long)]
    pub repo: Option<String>,
    /// Verbs to grant, e.g. read, write, land (repeatable or comma-separated).
    #[arg(long, value_delimiter = ',', required = true)]
    pub verbs: Vec<String>,
    /// Path prefixes the verbs apply to (default: the whole repository).
    #[arg(long, value_delimiter = ',')]
    pub paths: Vec<String>,
    /// Expiry as RFC 3339, or a duration like 30m, 12h, 7d.
    #[arg(long)]
    pub expires: Option<String>,
    /// Principal the token acts as: `name`, `agent:name` or `svc:name`.
    #[arg(long, value_name = "PRINCIPAL")]
    pub r#for: Option<String>,
}

/// `rill admin relation <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum RelationCmd {
    /// Add a tuple: <object> <relation> <subject>.
    Add(RelationArgs),
    /// Remove a tuple.
    Remove(RelationArgs),
    /// List tuples on an object.
    List(RelationListArgs),
}

/// A relation tuple.
#[derive(Args, Debug)]
pub struct RelationArgs {
    /// Object, e.g. `repo:main` or `path:main/src/auth`.
    pub object: String,
    /// Relation, e.g. `owner`, `reviewer`, `member`.
    pub relation: String,
    /// Subject, e.g. `principal:pr_...` or `team:infra#member`.
    pub subject: String,
}

/// `rill admin relation list`.
#[derive(Args, Debug)]
pub struct RelationListArgs {
    /// Object.
    pub object: String,
    /// Only this relation.
    #[arg(long)]
    pub relation: Option<String>,
}

/// `rill admin policy <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum PolicyCmd {
    /// Print a policy's text.
    Get(PolicyGetArgs),
    /// Replace a policy from a file (or stdin with `-`).
    Set(PolicySetArgs),
}

/// `rill admin policy get`.
#[derive(Args, Debug)]
pub struct PolicyGetArgs {
    /// Policy name.
    pub name: String,
}

/// `rill admin policy set`.
#[derive(Args, Debug)]
pub struct PolicySetArgs {
    /// Policy name.
    pub name: String,
    /// File holding the Cedar text (`-` for stdin).
    pub file: PathBuf,
}

/// Parse `30m`, `12h`, `7d` or RFC 3339 into a timestamp.
pub fn parse_expiry(text: &str, now: Timestamp) -> Result<Timestamp> {
    if let Some(t) = Timestamp::parse_rfc3339(text) {
        return Ok(t);
    }
    let (digits, unit) = text.split_at(
        text.trim_end_matches(|c: char| c.is_ascii_alphabetic())
            .len(),
    );
    let n: i64 = digits.parse().map_err(|_| {
        CliError::usage(format!(
            "invalid expiry {text:?}: use RFC 3339 or 30m/12h/7d"
        ))
    })?;
    let secs = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        "d" => n * 86_400,
        _ => return Err(CliError::usage(format!("invalid expiry unit in {text:?}"))),
    };
    Ok(Timestamp::from_millis(now.millis() + secs * 1000))
}

/// Run an admin subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: AdminCmd) -> Result<()> {
    match cmd {
        AdminCmd::Whoami => whoami(ctx).await,
        AdminCmd::Token(TokenCmd::Issue(args)) => issue(ctx, args).await,
        AdminCmd::Relation(RelationCmd::Add(args)) => relation_add(ctx, args, true).await,
        AdminCmd::Relation(RelationCmd::Remove(args)) => relation_add(ctx, args, false).await,
        AdminCmd::Relation(RelationCmd::List(args)) => relation_list(ctx, args).await,
        AdminCmd::Policy(PolicyCmd::Get(args)) => policy_get(ctx, args).await,
        AdminCmd::Policy(PolicyCmd::Set(args)) => policy_set(ctx, args).await,
        AdminCmd::Health => health(ctx).await,
    }
}

async fn whoami(ctx: &mut Ctx<'_>) -> Result<()> {
    let me = ctx.client()?.admin().whoami().await?;
    ctx.emit(json::whoami(&me), |out| {
        out.kv(&[
            (
                "principal",
                format!("{} ({})", me.principal, me.principal.id),
            ),
            ("kind", me.principal.kind.as_str().into()),
            ("teams", me.teams.join(", ")),
            (
                "expires",
                me.token_expires_at.map(|t| when(&t)).unwrap_or_default(),
            ),
        ])?;
        if !me.capabilities.is_empty() {
            let mut table = Table::new(&["ACTION", "SCOPE", "EXPIRES"]);
            for c in &me.capabilities {
                table.row(vec![
                    c.action.clone(),
                    c.scope.clone(),
                    c.expires_at.map(|t| when(&t)).unwrap_or_default(),
                ]);
            }
            out.table(table)?;
        }
        Ok(())
    })
}

async fn issue(ctx: &mut Ctx<'_>, args: IssueArgs) -> Result<()> {
    let repo = match args.repo {
        Some(r) => r,
        None => ctx.repo_name()?,
    };
    let paths = if args.paths.is_empty() {
        vec![String::new()]
    } else {
        args.paths
    };
    let expires_at = args
        .expires
        .as_deref()
        .map(|e| parse_expiry(e, Timestamp::now()))
        .transpose()?;
    let capabilities = args
        .verbs
        .iter()
        .flat_map(|verb| {
            let repo = repo.clone();
            paths.iter().map(move |path| {
                let scope = if path.is_empty() {
                    format!("repo:{repo}/*")
                } else {
                    format!("repo:{repo}/{}/*", path.trim_matches('/'))
                };
                Capability {
                    action: format!("repo.{verb}"),
                    scope,
                    expires_at,
                }
            })
        })
        .collect();
    let request = TokenRequest {
        capabilities,
        expires_at,
        for_principal: args.r#for.as_deref().map(super::ws::parse_principal),
    };
    let token = ctx.client()?.admin().issue_token(request).await?;
    ctx.emit(json::issued_token(&token), |out| {
        out.line(token.token.clone())?;
        out.line(out.dim(&format!("expires {}", when(&token.expires_at))))
    })
}

async fn relation_add(ctx: &mut Ctx<'_>, args: RelationArgs, add: bool) -> Result<()> {
    let relation = Relation::new(args.object, args.relation, args.subject);
    let admin = ctx.client()?.admin();
    if add {
        admin.add_relation(&relation).await?;
    } else {
        admin.remove_relation(&relation).await?;
    }
    ctx.emit(
        j!({ "relation": json::relation(&relation), "added": add }),
        |out| {
            out.line(format!(
                "{} {} {} {}",
                if add { "added" } else { "removed" },
                relation.object,
                relation.relation,
                relation.subject
            ))
        },
    )
}

async fn relation_list(ctx: &mut Ctx<'_>, args: RelationListArgs) -> Result<()> {
    let relations = ctx
        .client()?
        .admin()
        .list_relations(&args.object, args.relation.as_deref())
        .await?;
    ctx.emit(
        j!({ "relations": relations.iter().map(json::relation).collect::<Vec<_>>() }),
        |out| {
            if relations.is_empty() {
                return out.line(out.dim("no relations"));
            }
            let mut table = Table::new(&["OBJECT", "RELATION", "SUBJECT"]);
            for r in &relations {
                table.row(vec![
                    r.object.clone(),
                    r.relation.clone(),
                    r.subject.clone(),
                ]);
            }
            out.table(table)
        },
    )
}

async fn policy_get(ctx: &mut Ctx<'_>, args: PolicyGetArgs) -> Result<()> {
    let policy = ctx.client()?.admin().policy(&args.name).await?;
    ctx.emit(json::policy(&policy), |out| {
        out.line(out.dim(&format!(
            "// {} updated {} by {}",
            policy.name,
            when(&policy.updated_at),
            policy.updated_by
        )))?;
        out.raw(policy.text.as_bytes())?;
        if !policy.text.ends_with('\n') {
            out.blank()?;
        }
        Ok(())
    })
}

async fn policy_set(ctx: &mut Ctx<'_>, args: PolicySetArgs) -> Result<()> {
    let text = if args.file.as_os_str() == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
        s
    } else {
        let path = args.file.clone();
        tokio::task::spawn_blocking(move || std::fs::read_to_string(path))
            .await
            .map_err(|e| CliError::Other(anyhow::anyhow!("read task failed: {e}")))??
    };
    let updated = ctx.client()?.admin().set_policy(&args.name, &text).await?;
    ctx.emit(
        j!({ "name": args.name, "updated_at": json::timestamp(&updated) }),
        |out| {
            out.line(format!(
                "policy {} updated at {}",
                args.name,
                when(&updated)
            ))
        },
    )
}

async fn health(ctx: &mut Ctx<'_>) -> Result<()> {
    let health = ctx.client()?.admin().health().await?;
    let ok = health.ok;
    ctx.emit(json::health(&health), |out| {
        out.line(format!(
            "{} (version {})",
            if health.ok {
                out.good("healthy")
            } else {
                out.bad("unhealthy")
            },
            health.version
        ))?;
        for c in &health.components {
            out.line(format!(
                "  {:<16} {} {}",
                c.name,
                if c.ok {
                    out.good("ok")
                } else {
                    out.bad("FAIL")
                },
                c.detail.clone().unwrap_or_default()
            ))?;
        }
        Ok(())
    })?;
    if ok {
        Ok(())
    } else {
        Err(CliError::Other(anyhow::anyhow!(
            "server reports unhealthy components"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_durations_and_timestamps() {
        let now = Timestamp::from_millis(0);
        assert_eq!(parse_expiry("30m", now).unwrap().millis(), 30 * 60 * 1000);
        assert_eq!(parse_expiry("2d", now).unwrap().millis(), 2 * 86_400 * 1000);
        assert_eq!(
            parse_expiry("1970-01-01T00:00:10Z", now).unwrap().millis(),
            10_000
        );
        assert!(parse_expiry("soon", now).is_err());
    }
}
