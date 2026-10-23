//! Read-only tree commands: ls, cat, grep.

use clap::Args;
use rill_mcp::grep::{grep as grep_tree, GrepOptions};
use rill_mcp::json;
use rill_types::{EntryKind, RepoPathBuf};
use serde_json::{json as j, Value};

use crate::commands::root_arg;
use crate::context::Ctx;
use crate::Result;

/// `rill ls`.
#[derive(Args, Debug)]
pub struct LsArgs {
    /// Directory (default: the repository root).
    pub path: Option<String>,
    /// Root to list at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
}

/// `rill cat`.
#[derive(Args, Debug)]
pub struct CatArgs {
    /// File path.
    pub path: String,
    /// Root to read at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
}

/// `rill grep`.
#[derive(Args, Debug)]
pub struct GrepArgs {
    /// Regular expression.
    pub pattern: String,
    /// Directory to search under (default: the repository root).
    pub path: Option<String>,
    /// Root to search at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
    /// Case-insensitive matching.
    #[arg(short = 'i', long)]
    pub ignore_case: bool,
    /// Only files matching this glob, e.g. '*.rs'.
    #[arg(long)]
    pub glob: Option<String>,
    /// Stop after this many matches.
    #[arg(long, default_value_t = 1000)]
    pub max_matches: usize,
}

/// Run `rill ls`.
pub async fn ls(ctx: &mut Ctx<'_>, args: LsArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let path = match &args.path {
        Some(p) => RepoPathBuf::new(p)?,
        None => RepoPathBuf::root(),
    };
    let listing = repo.list_dir_full(&root, path.as_path()).await?;
    ctx.emit(
        j!({
            "root": root.to_string(),
            "path": path.as_str(),
            "tree_hash": listing.tree_hash.to_hex(),
            "entries": listing.entries.iter().map(|(n, k)| json::dir_entry(n, k)).collect::<Vec<_>>(),
        }),
        |out| {
            for (name, kind) in &listing.entries {
                match kind {
                    EntryKind::Dir { summary, .. } => out.line(format!(
                        "{:>10}  {}/",
                        out.dim(&format!("{} files", summary.file_count)),
                        out.bold(name)
                    ))?,
                    EntryKind::File { content, kind } => out.line(format!(
                        "{:>10}  {}{}",
                        content.size(),
                        name,
                        match kind {
                            rill_types::FileKind::Executable => "*",
                            rill_types::FileKind::Symlink => "@",
                            rill_types::FileKind::Regular => "",
                        }
                    ))?,
                }
            }
            Ok(())
        },
    )
}

/// Run `rill cat`.
pub async fn cat(ctx: &mut Ctx<'_>, args: CatArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let path = RepoPathBuf::new(&args.path)?;
    let data = repo.read_file(&root, path.as_path()).await?;
    if ctx.json() {
        let mut value = json::bytes(&data);
        value["path"] = Value::String(path.into_string());
        value["root"] = Value::String(root.to_string());
        return ctx.emit(value, |_| Ok(()));
    }
    ctx.out().raw(&data)?;
    ctx.out().flush()?;
    Ok(())
}

/// Run `rill grep`.
pub async fn grep(ctx: &mut Ctx<'_>, args: GrepArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let opts = GrepOptions {
        ignore_case: args.ignore_case,
        glob: args.glob.clone(),
        path: match &args.path {
            Some(p) => RepoPathBuf::new(p)?,
            None => RepoPathBuf::root(),
        },
        max_matches: args.max_matches,
        ..GrepOptions::default()
    };
    let result = grep_tree(&repo, &root, &args.pattern, &opts).await?;
    let mut value = result.to_json();
    value["root"] = Value::String(root.to_string());
    ctx.emit(value, |out| {
        for m in &result.matches {
            out.line(format!(
                "{}:{}: {}",
                out.bold(m.path.as_str()),
                out.dim(&m.line_no.to_string()),
                m.line
            ))?;
        }
        if result.truncated {
            out.line(out.warn("(search truncated: too many files, bytes or matches)"))?;
        }
        Ok(())
    })
}
