//! `rill show`: one version like `git show`: who, when, where it landed,
//! the message, a diffstat and the patch against its first parent.

use clap::Args;
use rill_mcp::json;
use rill_sdk::{DiffFormat, Root};
use rill_types::Tree;

use super::diff::root_diff;
use crate::commands::{change_ref, ChangeRef};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{ago, when, PatchMode, Role};
use crate::Result;

/// `rill show`.
#[derive(Args, Debug)]
pub struct ShowArgs {
    /// A version hash or a change id (its current version). Defaults to the
    /// checkout's working change, or the trunk head outside a checkout.
    pub change: Option<String>,
    /// The diffstat only, no patch.
    #[arg(long, conflicts_with_all = ["no_patch", "name_status"])]
    pub stat: bool,
    /// Paths with a status letter only, no patch.
    #[arg(long, conflicts_with = "no_patch")]
    pub name_status: bool,
    /// The header and message only.
    #[arg(long, short = 's')]
    pub no_patch: bool,
}

/// Run `rill show`.
pub async fn show(ctx: &mut Ctx<'_>, args: ShowArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let target = match args.change.as_deref() {
        Some(text) => change_ref(text)?,
        None => match ctx.working_change().await? {
            Some(id) => ChangeRef::Id(id),
            None => match repo.trunk_head().await? {
                Some((_, head)) => ChangeRef::Version(head),
                None => return Err(CliError::usage("trunk is empty; pass a change id")),
            },
        },
    };
    let info = target.info(&repo).await?;
    let version = repo.change_version(info.hash).await?;
    let files = if ctx.json() || args.no_patch {
        Vec::new()
    } else {
        let base = match version.parents.first() {
            Some(p) => Root::Version(*p),
            None => Root::Tree(Tree::empty_hash()),
        };
        root_diff(
            &repo,
            &base,
            &Root::Version(info.hash),
            vec![],
            3,
            DiffFormat::Unified,
        )
        .await?
        .1
    };
    let mut value = json::change_version(&version);
    value["info"] = json::change_info(&info);
    ctx.emit(value, |out| {
        let header = format!("change {}", info.change_id);
        out.line(format!(
            "{}  {}",
            out.paint(Role::Id, &header),
            out.paint(Role::Dim, &format!("version {}", info.hash.to_hex()))
        ))?;
        let field = |name: &str| format!("{name:<11}");
        out.line(format!(
            "{}{}",
            field("Author:"),
            out.paint(Role::Author, &info.author.to_string())
        ))?;
        out.line(format!(
            "{}{} {}",
            field("Date:"),
            out.paint(Role::Date, &ago(&info.created_at)),
            out.paint(Role::Dim, &format!("({})", when(&info.created_at)))
        ))?;
        let landed = match info.trunk_seq {
            Some(s) => out.paint(Role::Ref, &format!("trunk@{}", s.get())),
            None => out.paint(Role::Warning, "not landed"),
        };
        out.line(format!("{}{landed}", field("Landed:")))?;
        if let Some(intent) = info.intent {
            out.line(format!("{}{intent}", field("Intent:")))?;
        }
        let provenance = [
            version.provenance.tool.as_deref(),
            version.provenance.model.as_deref(),
            version.provenance.session.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" / ");
        if !provenance.is_empty() {
            out.line(format!("{}{provenance}", field("Via:")))?;
        }
        if let Some(by) = version.extra.get(rill_core::merge::STRUCTURAL_MERGE_EXTRA) {
            out.line(format!(
                "{}{}",
                field("Merged by:"),
                String::from_utf8_lossy(by)
            ))?;
        }
        if version.has_conflicts() {
            let painted = out.paint(Role::Conflict, "this version has unresolved conflicts");
            out.line(painted)?;
        }
        out.blank()?;
        for line in version.message.lines() {
            out.line(format!("    {line}"))?;
        }
        if args.no_patch {
            return Ok(());
        }
        out.blank()?;
        if files.is_empty() {
            return out.line(out.dim("(no file changes)"));
        }
        let mode = match (args.stat, args.name_status) {
            (true, _) => PatchMode::Stat,
            (_, true) => PatchMode::NameStatus,
            _ => {
                out.patch(&files, PatchMode::Stat)?;
                out.blank()?;
                PatchMode::Full
            }
        };
        out.patch(&files, mode)
    })
}
