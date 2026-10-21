//! `rill access`: the relationship tuples of a repository and questions
//! about them. The repository is `--repo`, `RILL_REPO` or the checkout's.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_sdk::{Relation, RelationFilter};
use serde_json::json as j;

use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill access <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum AccessCmd {
    /// Give a subject a relation on a directory (or the repository).
    Grant(TupleArgs),
    /// Take it away.
    Revoke(TupleArgs),
    /// The repository's tuples.
    List(ListArgs),
    /// Whether you (or `--as` someone) hold a relation, and why.
    Check(CheckArgs),
    /// Everyone holding a relation on a directory.
    Who(WhoArgs),
    /// The directories where you (or `--as` someone) hold a relation.
    Where(WhereArgs),
}

/// A tuple.
#[derive(Args, Debug)]
pub struct TupleArgs {
    /// `user:<name>`, `agent:<name>`, `team:<name>`, `kind:<kind>` or `*`.
    pub subject: String,
    /// `owner`, `writer`, `reviewer` or `reader`.
    pub relation: String,
    /// A directory; the whole repository when omitted.
    #[arg(default_value = "")]
    pub path: String,
}

/// `rill access list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only tuples on this directory.
    #[arg(long)]
    pub path: Option<String>,
    /// Only tuples of this subject.
    #[arg(long)]
    pub subject: Option<String>,
    /// Only this relation.
    #[arg(long)]
    pub relation: Option<String>,
}

/// `rill access check`.
#[derive(Args, Debug)]
pub struct CheckArgs {
    /// `owner`, `writer`, `reviewer` or `reader`.
    pub relation: String,
    /// A directory; the whole repository when omitted.
    #[arg(default_value = "")]
    pub path: String,
    /// `user:<name>` or `agent:<name>` (needs owner).
    #[arg(long = "as")]
    pub subject: Option<String>,
}

/// `rill access who`.
#[derive(Args, Debug)]
pub struct WhoArgs {
    /// `owner`, `writer`, `reviewer` or `reader`.
    pub relation: String,
    /// A directory; the whole repository when omitted.
    #[arg(default_value = "")]
    pub path: String,
}

/// `rill access where`.
#[derive(Args, Debug)]
pub struct WhereArgs {
    /// `owner`, `writer`, `reviewer` or `reader`.
    pub relation: String,
    /// `user:<name>` or `agent:<name>` (needs owner).
    #[arg(long = "as")]
    pub subject: Option<String>,
}

fn place(path: &str) -> String {
    if path.is_empty() {
        "the repository".into()
    } else {
        format!("{path}/")
    }
}

/// Run an access subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: AccessCmd) -> Result<()> {
    let repo = ctx.repo_name()?;
    let access = ctx.client()?.access();
    match cmd {
        AccessCmd::Grant(a) => {
            let tuple = Relation::new(repo, a.path, a.relation, a.subject);
            access.grant(&tuple).await?;
            ctx.emit(json::relation(&tuple), |out| {
                out.line(format!(
                    "{} is now {} of {}",
                    tuple.subject,
                    tuple.relation,
                    place(&tuple.path)
                ))
            })
        }
        AccessCmd::Revoke(a) => {
            let tuple = Relation::new(repo, a.path, a.relation, a.subject);
            access.revoke(&tuple).await?;
            ctx.emit(json::relation(&tuple), |out| {
                out.line(format!(
                    "{} is no longer {} of {}",
                    tuple.subject,
                    tuple.relation,
                    place(&tuple.path)
                ))
            })
        }
        AccessCmd::List(a) => {
            let filter = RelationFilter {
                path: a.path,
                subject: a.subject,
                relation: a.relation,
            };
            let tuples = access.list(&repo, filter).await?;
            ctx.emit(
                j!({ "relations": tuples.iter().map(json::relation).collect::<Vec<_>>() }),
                |out| relation_table(out, &tuples),
            )
        }
        AccessCmd::Check(a) => {
            let check = access
                .check(&repo, &a.path, &a.relation, a.subject.as_deref())
                .await?;
            ctx.emit(
                j!({
                    "allowed": check.allowed,
                    "org_admin": check.org_admin,
                    "because": check.because.iter().map(json::relation).collect::<Vec<_>>(),
                }),
                |out| {
                    let who = a.subject.as_deref().unwrap_or("you");
                    if !check.allowed {
                        return out.line(format!(
                            "{who}: not {} of {}",
                            a.relation,
                            place(&a.path)
                        ));
                    }
                    out.line(format!("{who}: {} of {}", a.relation, place(&a.path)))?;
                    if check.org_admin {
                        out.line(out.dim("  as an administrator of the organization"))?;
                    }
                    for t in &check.because {
                        out.line(out.dim(&format!(
                            "  {} is {} of {}",
                            t.subject,
                            t.relation,
                            place(&t.path)
                        )))?;
                    }
                    Ok(())
                },
            )
        }
        AccessCmd::Who(a) => {
            let expanded = access.expand(&repo, &a.path, &a.relation).await?;
            ctx.emit(
                j!({ "relations": expanded.iter().map(|e| j!({
                    "relation": json::relation(&e.relation),
                    "members": e.members.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
                })).collect::<Vec<_>>() }),
                |out| {
                    let mut table = Table::new(&["SUBJECT", "RELATION", "ON", "MEMBERS"]);
                    for e in &expanded {
                        table.row(vec![
                            e.relation.subject.clone(),
                            e.relation.relation.clone(),
                            place(&e.relation.path),
                            e.members
                                .iter()
                                .map(|m| m.name.clone())
                                .collect::<Vec<_>>()
                                .join(", "),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        AccessCmd::Where(a) => {
            let paths = access
                .lookup_paths(&repo, &a.relation, a.subject.as_deref())
                .await?;
            ctx.emit(j!({ "paths": paths }), |out| {
                if paths.is_empty() {
                    return out.line(out.dim("nowhere"));
                }
                for p in &paths {
                    out.line(place(p))?;
                }
                Ok(())
            })
        }
    }
}

fn relation_table(out: &mut crate::output::Out<'_>, tuples: &[Relation]) -> std::io::Result<()> {
    if tuples.is_empty() {
        return out.line(out.dim("no relations"));
    }
    let mut table = Table::new(&["SUBJECT", "RELATION", "ON"]);
    for t in tuples {
        table.row(vec![t.subject.clone(), t.relation.clone(), place(&t.path)]);
    }
    out.table(table)
}
