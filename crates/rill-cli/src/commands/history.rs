//! Read-only history and tree commands: log, blame, show, ls, cat, grep.

use clap::Args;
use rill_mcp::grep::{grep as grep_tree, GrepOptions};
use rill_mcp::json;
use rill_sdk::LogStart;
use rill_types::{ChangeInfo, EntryKind, RepoPathBuf};
use serde_json::{json as j, Value};

use crate::commands::{change_ref, root_arg};
use crate::context::Ctx;
use crate::output::{short, when, Out, Table};
use crate::Result;

/// `rill log`.
#[derive(Args, Debug)]
pub struct LogArgs {
    /// Where to start: trunk (default), trunk@N, a version hash or a ref.
    pub root: Option<String>,
    /// Maximum entries.
    #[arg(long, short = 'n', default_value_t = 20)]
    pub limit: u32,
    /// Only versions that touched this path.
    #[arg(long)]
    pub path: Option<String>,
    /// Follow only first parents.
    #[arg(long)]
    pub first_parent: bool,
}

/// `rill blame`.
#[derive(Args, Debug)]
pub struct BlameArgs {
    /// File path.
    pub path: String,
    /// Root to blame at (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
}

/// `rill show`.
#[derive(Args, Debug)]
pub struct ShowArgs {
    /// A version hash or a change id (its current version).
    pub change: String,
}

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

/// Print one log row.
pub fn log_row(out: &Out<'_>, c: &ChangeInfo) -> Vec<String> {
    let seq = c
        .trunk_seq
        .map(|s| format!("trunk@{}", s.get()))
        .unwrap_or_default();
    let flags = if c.has_conflicts {
        out.bad("conflict")
    } else {
        String::new()
    };
    vec![
        out.id(&short(&c.hash)),
        seq,
        c.author.to_string(),
        when(&c.created_at),
        format!(
            "{}{}",
            c.summary(),
            if flags.is_empty() {
                String::new()
            } else {
                format!(" [{flags}]")
            }
        ),
    ]
}

/// Run `rill log`.
pub async fn log(ctx: &mut Ctx<'_>, args: LogArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let start: LogStart = root_arg(args.root.as_deref())?.into();
    let path = args.path.as_deref().map(RepoPathBuf::new).transpose()?;
    let page = repo
        .log_page(start, args.limit, path.as_deref(), args.first_parent)
        .await?;
    ctx.emit(json::log_page(&page), |out| {
        if page.entries.is_empty() {
            return out.line(out.dim("no history"));
        }
        let mut table = Table::new(&["VERSION", "SEQ", "AUTHOR", "WHEN", "SUMMARY"]);
        for c in &page.entries {
            table.row(log_row(out, c));
        }
        out.table(table)?;
        if let Some(next) = page.next_start {
            out.line(out.dim(&format!("(more: rill log {})", next.to_hex())))?;
        }
        Ok(())
    })
}

/// Run `rill blame`.
pub async fn blame(ctx: &mut Ctx<'_>, args: BlameArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let root = root_arg(args.at.as_deref())?;
    let path = RepoPathBuf::new(&args.path)?;
    let hash = repo.resolve_root(&root).await?;
    let lines = repo.blame(hash, path.as_path()).await?;
    ctx.emit(
        j!({
            "path": path.as_str(),
            "version": hash.to_hex(),
            "lines": lines.iter().map(json::blame_line).collect::<Vec<_>>(),
        }),
        |out| {
            let width = lines.len().to_string().len();
            for l in &lines {
                let seq = l
                    .trunk_seq
                    .map(|s| format!("@{}", s.get()))
                    .unwrap_or_default();
                out.line(format!(
                    "{} {:<12}{:>6} {:>width$} | {}",
                    out.id(&short(&l.version)),
                    l.author.to_string(),
                    out.dim(&seq),
                    l.line_no,
                    String::from_utf8_lossy(&l.text),
                ))?;
            }
            Ok(())
        },
    )
}

/// Run `rill show`.
pub async fn show(ctx: &mut Ctx<'_>, args: ShowArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let info = change_ref(&args.change)?.info(&repo).await?;
    let version = repo.change_version(info.hash).await?;
    let mut value = json::change_version(&version);
    value["info"] = json::change_info(&info);
    ctx.emit(value, |out| {
        out.kv(&[
            ("version", out.id(&info.hash.to_hex())),
            ("change", info.change_id.to_string()),
            (
                "parents",
                version
                    .parents
                    .iter()
                    .map(short)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            (
                "predecessor",
                version.predecessor.map(|h| short(&h)).unwrap_or_default(),
            ),
            ("author", info.author.to_string()),
            ("created", when(&info.created_at)),
            (
                "trunk",
                info.trunk_seq
                    .map(|s| format!("trunk@{}", s.get()))
                    .unwrap_or_else(|| "not landed".into()),
            ),
            (
                "intent",
                info.intent.map(|i| i.to_string()).unwrap_or_default(),
            ),
            (
                "provenance",
                [
                    version.provenance.tool.as_deref(),
                    version.provenance.model.as_deref(),
                    version.provenance.session.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" / "),
            ),
        ])?;
        out.blank()?;
        for line in version.message.lines() {
            out.line(format!("    {line}"))?;
        }
        out.blank()?;
        for (path, change) in &version.file_changes {
            let tag = match change {
                rill_types::FileChange::Set { .. } => out.good("set"),
                rill_types::FileChange::Delete => out.bad("delete"),
                rill_types::FileChange::Conflict(_) => out.bad("CONFLICT"),
            };
            out.line(format!("  {tag:<10} {path}"))?;
        }
        if version.file_changes.is_empty() {
            out.line(out.dim("  (no file changes)"))?;
        }
        Ok(())
    })
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
