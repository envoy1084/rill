//! `rill resolve`: marking conflicted paths resolved.

use rill_sdk::LocalChangeKind;
use rill_types::RepoPathBuf;
use serde_json::json as j;

use super::args::ResolveArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// Run `rill resolve`.
pub async fn resolve(ctx: &mut Ctx<'_>, args: ResolveArgs) -> Result<()> {
    let mut checkout = ctx.checkout().await?;
    let conflicted: Vec<RepoPathBuf> = checkout
        .status()
        .await?
        .into_iter()
        .filter(|c| c.kind == LocalChangeKind::Conflict)
        .map(|c| c.path)
        .collect();
    // With --list, or with nothing to mark, show what is conflicted and how
    // to mark it rather than silently doing nothing.
    if args.list || (args.paths.is_empty() && !args.all) {
        return ctx.emit(
            j!({ "conflicts": conflicted.iter().map(|p| p.as_str()).collect::<Vec<_>>() }),
            |out| {
                if conflicted.is_empty() {
                    return out.line(out.dim("no conflicted paths"));
                }
                for p in &conflicted {
                    out.line(format!("{} {p}", out.bad("C")))?;
                }
                out.line(
                    out.dim(
                        "edit the markers, then `rill resolve <paths>` or `rill resolve --all`",
                    ),
                )
            },
        );
    }
    let paths: Vec<RepoPathBuf> = if args.all {
        conflicted.clone()
    } else {
        args.paths
            .iter()
            .map(RepoPathBuf::new)
            .collect::<std::result::Result<_, _>>()?
    };
    for p in &paths {
        if !conflicted.contains(p) {
            return Err(CliError::usage(format!("{p} is not conflicted")));
        }
    }
    checkout.mark_resolved(&paths, args.force).await?;
    let remaining = conflicted.len() - paths.len();
    ctx.emit(
        j!({
            "resolved": paths.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "remaining": remaining,
        }),
        |out| {
            out.line(format!("marked {} path(s) resolved", paths.len()))?;
            if remaining > 0 {
                out.line(out.warn(&format!("{remaining} still conflicted")))?;
            }
            Ok(())
        },
    )
}
