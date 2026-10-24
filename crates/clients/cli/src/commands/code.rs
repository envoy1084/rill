//! Code intelligence: `rill def`, `rill uses`, `rill symbols` and
//! `rill scip upload`.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_sdk::{ReferenceTarget, SymbolLocation};
use rill_types::RepoPathBuf;
use serde_json::{json as j, Value};

use crate::commands::root_arg;
use crate::context::Ctx;
use crate::output::Out;
use crate::{CliError as Error, Result};

/// `rill def`.
#[derive(Args, Debug)]
pub struct DefArgs {
    /// Symbol name.
    pub name: String,
    /// Root to look at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
}

/// `rill uses`.
#[derive(Args, Debug)]
pub struct UsesArgs {
    /// A symbol name, or `PATH:LINE:COLUMN` (one-based line, zero-based
    /// column) for the symbol there.
    pub target: String,
    /// Root to look at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
    /// Stop after this many references.
    #[arg(long, default_value_t = 1000)]
    pub max: u32,
}

/// `rill symbols`.
#[derive(Args, Debug)]
pub struct SymbolsArgs {
    /// List this file's symbols.
    pub path: Option<String>,
    /// Symbols whose names contain this (case-insensitive).
    #[arg(long, conflicts_with = "path")]
    pub query: Option<String>,
    /// Only this kind (`function`, `class`, `struct`, …); repeatable.
    #[arg(long = "kind", value_name = "KIND")]
    pub kinds: Vec<String>,
    /// Root to look at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
}

/// `rill scip`.
#[derive(Subcommand, Debug)]
pub enum ScipCmd {
    /// Attach a SCIP index (`index.scip`) built from a root.
    Upload(ScipUploadArgs),
}

/// `rill scip upload`.
#[derive(Args, Debug)]
pub struct ScipUploadArgs {
    /// The encoded SCIP index file.
    pub file: std::path::PathBuf,
    /// The root the index was built from (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
    /// The directory the index's paths are relative to.
    #[arg(long, default_value = "")]
    pub prefix: String,
}

fn print(out: &mut Out<'_>, list: &[SymbolLocation]) -> std::io::Result<()> {
    for s in list {
        let mut tags = Vec::new();
        if !s.kind.is_empty() {
            tags.push(s.kind.clone());
        }
        if s.definition && s.kind.is_empty() {
            tags.push("definition".into());
        }
        if s.precise {
            tags.push("scip".into());
        }
        out.line(format!(
            "{}:{}: {} {}",
            out.bold(s.path.as_str()),
            out.dim(&s.line.to_string()),
            s.name,
            out.dim(&tags.join(" "))
        ))?;
    }
    Ok(())
}

/// Run `rill def`.
pub async fn def(ctx: &mut Ctx<'_>, args: DefArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let defs = repo.find_definitions(&root, &args.name).await?;
    let value = j!({
        "root": root.to_string(),
        "definitions": defs.iter().map(json::symbol_location).collect::<Vec<_>>(),
    });
    ctx.emit(value, |out| print(out, &defs))
}

/// Run `rill uses`.
pub async fn uses(ctx: &mut Ctx<'_>, args: UsesArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let target = target(&args.target)?;
    let found = repo.find_references(&root, &target, args.max).await?;
    let mut value = json::references(&found);
    value["root"] = Value::String(root.to_string());
    ctx.emit(value, |out| {
        print(out, &found.references)?;
        if !found.precise {
            out.line(out.dim("(text matches; upload a SCIP index for precise references)"))?;
        }
        if found.truncated {
            out.line(out.warn("(truncated at --max)"))?;
        }
        Ok(())
    })
}

/// A name, or `PATH:LINE:COLUMN`.
fn target(raw: &str) -> Result<ReferenceTarget> {
    let mut parts = raw.rsplitn(3, ':');
    let (column, line, path) = (parts.next(), parts.next(), parts.next());
    match (
        path,
        line.and_then(|l| l.parse().ok()),
        column.and_then(|c| c.parse().ok()),
    ) {
        (Some(path), Some(line), Some(column)) => Ok(ReferenceTarget::Position {
            path: RepoPathBuf::new(path)?,
            line,
            column,
        }),
        _ if raw.contains(':') && raw.contains('/') => Err(Error::usage(format!(
            "{raw:?}: give a name or PATH:LINE:COLUMN"
        ))),
        _ => Ok(ReferenceTarget::Name(raw.to_string())),
    }
}

/// Run `rill symbols`.
pub async fn symbols(ctx: &mut Ctx<'_>, args: SymbolsArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let list = match (&args.path, &args.query) {
        (Some(path), _) => {
            let path = RepoPathBuf::new(path)?;
            let all = repo.file_symbols(&root, path.as_path()).await?;
            all.into_iter()
                .filter(|s| {
                    args.kinds.is_empty()
                        || args.kinds.iter().any(|k| k.eq_ignore_ascii_case(&s.kind))
                })
                .collect()
        }
        (None, Some(q)) => repo.search_symbols(&root, q, &args.kinds, 0).await?,
        (None, None) => return Err(Error::usage("give a file or --query")),
    };
    let value = j!({
        "root": root.to_string(),
        "symbols": list.iter().map(json::symbol_location).collect::<Vec<_>>(),
    });
    ctx.emit(value, |out| print(out, &list))
}

/// Run `rill scip`.
pub async fn scip(ctx: &mut Ctx<'_>, cmd: ScipCmd) -> Result<()> {
    let ScipCmd::Upload(args) = cmd;
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let bytes = std::fs::read(&args.file)
        .map_err(|e| Error::usage(format!("{}: {e}", args.file.display())))?;
    let stored = repo.upload_scip(&root, bytes.into(), &args.prefix).await?;
    ctx.emit(
        j!({
            "root": root.to_string(),
            "files": stored.files,
            "occurrences": stored.occurrences,
        }),
        |out| {
            out.line(format!(
                "SCIP index attached: {} files, {} occurrences",
                stored.files, stored.occurrences
            ))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_names_or_positions() {
        assert_eq!(
            target("greet").unwrap(),
            ReferenceTarget::Name("greet".into())
        );
        assert_eq!(
            target("src/lib.rs:3:7").unwrap(),
            ReferenceTarget::Position {
                path: RepoPathBuf::new("src/lib.rs").unwrap(),
                line: 3,
                column: 7
            }
        );
        assert_eq!(
            target("Foo::bar").unwrap(),
            ReferenceTarget::Name("Foo::bar".into())
        );
        assert!(target("src/lib.rs:x:1").is_err());
    }
}
