//! Opening, listing and resolving review threads.

use rill_mcp::json;
use rill_sdk::{NewComment, Reply};
use rill_types::{RepoPathBuf, ThreadAnchor};
use serde_json::json as j;

use super::args::{ChangeArg, CommentArgs, ReplyArgs, ResolveThreadArgs};
use super::working_change::change_or_working;
use crate::commands::change_ref;
use crate::context::Ctx;
use crate::output::when;
use crate::Result;

/// Run `rill review comment` and `rill comment`.
pub async fn comment(ctx: &mut Ctx<'_>, args: CommentArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let anchor = match (&args.path, args.line) {
        (None, _) => ThreadAnchor::Change,
        (Some(path), line) => {
            let version = repo.current_version(id).await?.hash;
            let path = RepoPathBuf::new(path)?;
            match line {
                None => ThreadAnchor::File { version, path },
                Some(line) => ThreadAnchor::Lines {
                    version,
                    path,
                    start: line,
                    end: line,
                    context_hash: String::new(),
                },
            }
        }
    };
    let new = NewComment {
        anchor,
        body: args.message,
        category: None,
        confidence: None,
        to: None,
    };
    let thread = repo.reviews().comment(id, new).await?;
    ctx.emit(json::new_thread(&thread), |out| {
        out.line(format!("opened thread {}", out.id(&thread.thread_id)))
    })
}

pub(super) async fn reply(ctx: &mut Ctx<'_>, args: ReplyArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let reply = Reply {
        body: args.message,
        category: None,
        confidence: None,
    };
    let thread = repo.reviews().reply(id, &args.thread, reply).await?;
    ctx.emit(json::thread(&thread), |out| {
        out.line(format!("replied on thread {}", out.id(&thread.id)))
    })
}

pub(super) async fn threads(ctx: &mut Ctx<'_>, args: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let info = repo.reviews().get(id).await?;
    let threads = info.review.threads;
    ctx.emit(
        j!({ "change_id": id.to_string(), "threads": threads.iter().map(json::thread).collect::<Vec<_>>() }),
        |out| {
            if threads.is_empty() {
                return out.line(out.dim("no threads"));
            }
            for t in &threads {
                let state = if t.resolved {
                    out.good("resolved")
                } else {
                    out.warn("open")
                };
                let anchor = match &t.anchor {
                    ThreadAnchor::Change => String::new(),
                    ThreadAnchor::File { path, .. } => format!(" {path}"),
                    ThreadAnchor::Lines { path, start, end, .. } => format!(" {path}:{start}-{end}"),
                };
                out.line(format!("{} [{state}]{anchor}", out.id(&t.id)))?;
                for c in &t.comments {
                    out.line(format!("    {} {}: {}", out.dim(&when(&c.created_at)), c.author, c.body))?;
                }
            }
            Ok(())
        },
    )
}

pub(super) async fn resolve_thread(ctx: &mut Ctx<'_>, args: ResolveThreadArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let thread = repo
        .reviews()
        .resolve_thread(id, &args.thread, !args.reopen)
        .await?;
    ctx.emit(json::thread(&thread), |out| {
        out.line(format!(
            "thread {} {}",
            out.id(&thread.id),
            if thread.resolved {
                "resolved"
            } else {
                "reopened"
            }
        ))
    })
}
