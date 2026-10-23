//! `rill log`: history as a graph, jj-style. Inside a checkout the
//! working change and the unlanded changes under it are drawn on their own
//! lane, joining trunk where they are based.

use clap::Args;
use rill_mcp::json;
use rill_sdk::{LogStart, RepoClient};
use rill_types::{ChangeInfo, RepoPathBuf};

use crate::commands::root_arg;
use crate::context::Ctx;
use crate::output::graph::{self, Cell, Node, NodeKind};
use crate::output::{ago, short, when, Out, Role};
use crate::Result;

/// Unlanded changes followed below the working change.
const MAX_STACK: usize = 50;

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
    /// Continue from a previous page's `next_page_token`.
    #[arg(long)]
    pub page_token: Option<String>,
    /// One short line per version: id and summary.
    #[arg(long)]
    pub oneline: bool,
    /// No graph, and not the checkout's unlanded changes.
    #[arg(long)]
    pub no_graph: bool,
}

/// Run `rill log`.
pub async fn log(ctx: &mut Ctx<'_>, args: LogArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let start: LogStart = root_arg(args.root.as_deref())?.into();
    let path = args.path.as_deref().map(RepoPathBuf::new).transpose()?;
    let page = repo
        .log_page(
            start,
            args.limit,
            path.as_deref(),
            args.first_parent,
            args.page_token.as_deref(),
        )
        .await?;
    let plain_history = args.root.is_some() || path.is_some() || args.page_token.is_some();
    let stack = if plain_history || args.no_graph {
        Vec::new()
    } else {
        match ctx.working_change().await? {
            Some(id) => unlanded_stack(&repo, id).await?,
            None => Vec::new(),
        }
    };
    let mut value = json::log_page(&page);
    value["stack"] = serde_json::Value::Array(stack.iter().map(json::change_info).collect());
    ctx.emit(value, |out| {
        if page.entries.is_empty() && stack.is_empty() {
            return out.line(out.dim("no history"));
        }
        let mut entries: Vec<(&ChangeInfo, NodeKind)> = Vec::new();
        for (i, c) in stack.iter().enumerate() {
            let kind = if i == 0 {
                NodeKind::Working
            } else {
                NodeKind::Change
            };
            entries.push((c, kind));
        }
        for c in &page.entries {
            if !stack.iter().any(|s| s.hash == c.hash) {
                let kind = if c.trunk_seq.is_some() {
                    NodeKind::Trunk
                } else {
                    NodeKind::Change
                };
                entries.push((c, kind));
            }
        }
        draw(out, &entries, &args)?;
        if let Some(next) = &page.next_page_token {
            out.line(out.dim(&format!("(more: rill log --page-token {next})")))?;
        }
        Ok(())
    })
}

/// The working change's current version and the unlanded versions under
/// it, top first.
async fn unlanded_stack(repo: &RepoClient, id: rill_types::ChangeId) -> Result<Vec<ChangeInfo>> {
    let mut out = Vec::new();
    let mut next = match repo.current_version(id).await {
        Ok(info) if info.trunk_seq.is_none() => Some(info),
        Ok(_) => None,
        Err(e) if e.is_not_found() => None,
        Err(e) => return Err(e.into()),
    };
    while let Some(info) = next.take() {
        let parent = info.parents.first().copied();
        out.push(info);
        if out.len() >= MAX_STACK {
            break;
        }
        if let Some(p) = parent {
            let pinfo = repo.change_info(p).await?;
            if pinfo.trunk_seq.is_none() {
                next = Some(pinfo);
            }
        }
    }
    Ok(out)
}

fn cells(out: &Out<'_>, row: &[Cell]) -> String {
    row.iter()
        .map(|c| {
            let role = match c {
                Cell::Node(NodeKind::Working) => Role::GraphWorking,
                Cell::Node(NodeKind::Change) => Role::GraphChange,
                Cell::Node(NodeKind::Trunk) => Role::GraphTrunk,
                Cell::Node(NodeKind::Conflict) => Role::Conflict,
                _ => Role::GraphLine,
            };
            out.paint(role, &c.text())
        })
        .collect()
}

fn draw(
    out: &mut Out<'_>,
    entries: &[(&ChangeInfo, NodeKind)],
    args: &LogArgs,
) -> std::io::Result<()> {
    let nodes: Vec<Node> = entries
        .iter()
        .map(|(c, kind)| Node {
            id: c.hash.to_hex(),
            parent: c.parents.first().map(|p| p.to_hex()),
            kind: if c.has_conflicts {
                NodeKind::Conflict
            } else {
                *kind
            },
        })
        .collect();
    let rows = if args.no_graph {
        Vec::new()
    } else {
        graph::layout(&nodes)
    };
    for (i, (c, kind)) in entries.iter().enumerate() {
        let prefix = rows.get(i).map(|r| cells(out, &r.node)).unwrap_or_default();
        let label = match c.trunk_seq {
            Some(seq) => out.paint(Role::Ref, &format!("trunk@{}", seq.get())),
            None => out.paint(Role::Id, &c.change_id.to_string()[..11]),
        };
        let text = if args.oneline {
            format!("{label} {}", c.summary())
        } else {
            let mut decorations = Vec::new();
            if *kind == NodeKind::Working {
                decorations.push(out.paint(Role::GraphWorking, "(working change)"));
            }
            if c.has_conflicts {
                decorations.push(out.paint(Role::Conflict, "[conflict]"));
            }
            format!(
                "{label}  {}  {}  {}  {}{}",
                out.paint(Role::Dim, &short(&c.hash)[..8]),
                out.paint(Role::Author, &c.author.to_string()),
                out.paint(Role::Date, &ago(&c.created_at)),
                c.summary(),
                if decorations.is_empty() {
                    String::new()
                } else {
                    format!("  {}", decorations.join(" "))
                }
            )
        };
        out.line(format!("{prefix}{text}"))?;
        if let Some(r) = rows.get(i) {
            for after in &r.after {
                out.line(cells(out, after).trim_end())?;
            }
        }
    }
    Ok(())
}

/// A version as table cells: version, trunk position, author, time and
/// summary (for `rill stack` and other listings).
pub fn log_row(out: &Out<'_>, c: &ChangeInfo) -> Vec<String> {
    let seq = c
        .trunk_seq
        .map(|s| format!("trunk@{}", s.get()))
        .unwrap_or_default();
    let flags = if c.has_conflicts {
        format!(" [{}]", out.paint(Role::Conflict, "conflict"))
    } else {
        String::new()
    };
    vec![
        out.id(&short(&c.hash)),
        out.paint(Role::Ref, &seq),
        out.paint(Role::Author, &c.author.to_string()),
        out.paint(Role::Date, &when(&c.created_at)),
        format!("{}{flags}", c.summary()),
    ]
}
