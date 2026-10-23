//! `rill status`: the checkout's local changes, git-style.

use clap::Args;
use rill_daemon::proto as dpb;
use rill_daemon::{ChangeKind, LocalChange};
use rill_mcp::json;
use rill_types::RepoPathBuf;
use serde_json::{json as j, Value};

use crate::context::Ctx;
use crate::output::{Out, Role};
use crate::Result;

/// `rill status`.
#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Only paths under these prefixes.
    pub paths: Vec<String>,
    /// One line per path with a status letter, like `git status --short`.
    #[arg(long, short = 's')]
    pub short: bool,
}

/// Paths given on the command line, checked.
pub(crate) fn parse_paths(paths: &[String]) -> Result<Vec<RepoPathBuf>> {
    paths
        .iter()
        .map(|p| RepoPathBuf::new(p).map_err(Into::into))
        .collect()
}

/// Run `rill status`.
pub async fn status(ctx: &mut Ctx<'_>, args: StatusArgs) -> Result<()> {
    parse_paths(&args.paths)?;
    let mount = ctx.mount().await?;
    let reply = ctx
        .daemon()
        .await?
        .get_status(dpb::GetStatusRequest {
            name: mount.name.clone(),
            prefixes: args.paths.clone(),
        })
        .await?;
    let changes: Vec<LocalChange> = rill_daemon::client::local_changes(&reply.changes);
    let meta = reply.mount.into_option().unwrap_or(mount);
    let dir = ctx.find_checkout();
    let conflicts = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Conflict)
        .count();
    ctx.emit(
        j!({
            "repo": meta.repo,
            "base": meta.base,
            "base_version": non_empty(&meta.base_version),
            "working_change": non_empty(&meta.working_change),
            "workspace": non_empty(&meta.workspace),
            "pending_sync": meta.pending,
            "changes": changes.iter().map(local_change_json).collect::<Vec<_>>(),
            "conflicts": conflicts,
        }),
        |out| {
            if args.short {
                short(out, &changes)
            } else {
                long(out, &meta, &changes, dir.as_deref())
            }
        },
    )
}

/// The letter and role of a change kind.
pub(crate) fn letter(kind: ChangeKind) -> (&'static str, Role) {
    match kind {
        ChangeKind::Added => ("A", Role::Added),
        ChangeKind::Modified => ("M", Role::Modified),
        ChangeKind::Deleted => ("D", Role::Removed),
        ChangeKind::Conflict => ("C", Role::Conflict),
    }
}

fn short(out: &mut Out<'_>, changes: &[LocalChange]) -> std::io::Result<()> {
    for c in changes {
        let (l, role) = letter(c.kind);
        let painted = out.paint(role, l);
        out.line(format!("{painted} {}", c.path))?;
    }
    Ok(())
}

fn long(
    out: &mut Out<'_>,
    meta: &dpb::Mount,
    changes: &[LocalChange],
    dir: Option<&std::path::Path>,
) -> std::io::Result<()> {
    let linked = |out: &Out<'_>, path: &rill_types::RepoPathBuf| match dir {
        Some(d) => out.link(path.as_str(), &format!("file://{}/{}", d.display(), path)),
        None => path.to_string(),
    };
    let version = meta
        .base_version
        .get(..12)
        .map(|h| format!(" ({})", out.paint(Role::Id, h)))
        .unwrap_or_default();
    out.line(format!(
        "On {} at {}{version}",
        out.paint(Role::Bold, &meta.repo),
        out.paint(Role::Ref, &meta.base)
    ))?;
    if meta.working_change.is_empty() {
        out.line("No working change yet")?;
    } else {
        out.line(format!(
            "Working change {}",
            out.paint(Role::Id, &meta.working_change)
        ))?;
    }
    if !meta.sync_error.is_empty() {
        let line = format!("Sync to the server is failing: {}", meta.sync_error);
        let painted = out.paint(Role::Warning, &line);
        out.line(painted)?;
    } else if meta.pending > 0 {
        let line = format!(
            "{} edit{} not synced to the server yet",
            meta.pending,
            if meta.pending == 1 { "" } else { "s" }
        );
        let painted = out.paint(Role::Dim, &line);
        out.line(painted)?;
    }
    let (edits, conflicts): (Vec<&LocalChange>, Vec<&LocalChange>) =
        changes.iter().partition(|c| c.kind != ChangeKind::Conflict);
    if changes.is_empty() {
        out.blank()?;
        return out.line("Nothing to record; the working copy matches its base.");
    }
    if !edits.is_empty() {
        out.blank()?;
        out.line("Changes:")?;
        let hint = if meta.working_change.is_empty() {
            "  (`rill diff` to review, `rill new -m <message>` to record them)"
        } else {
            "  (`rill diff` to review, `rill amend` to record them)"
        };
        let painted = out.paint(Role::Hint, hint);
        out.line(painted)?;
        for c in &edits {
            let (label, role) = match c.kind {
                ChangeKind::Added => ("new file:", Role::Added),
                ChangeKind::Deleted => ("deleted:", Role::Removed),
                _ => ("modified:", Role::Modified),
            };
            let painted = out.paint(role, &format!("{label:<12}"));
            out.line(format!(
                "\t{painted}{}",
                out.paint(role, &linked(out, &c.path))
            ))?;
        }
    }
    if !conflicts.is_empty() {
        out.blank()?;
        out.line("Conflicts:")?;
        let painted = out.paint(
            Role::Hint,
            "  (edit the markers, then `rill resolve <path>`)",
        );
        out.line(painted)?;
        for c in &conflicts {
            let painted = out.paint(
                Role::Conflict,
                &format!("{:<12}{}", "both changed:", c.path),
            );
            out.line(format!("\t{painted}"))?;
        }
    }
    Ok(())
}

/// `{"path", "kind"}` of one local change.
pub(crate) fn local_change_json(c: &LocalChange) -> Value {
    j!({ "path": c.path.as_str(), "kind": json::enum_name(&c.kind) })
}

fn non_empty(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}
