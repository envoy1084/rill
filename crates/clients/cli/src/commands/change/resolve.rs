//! `rill conflicts` and `rill resolve`: listing conflicted paths and
//! marking them resolved.

use rill_daemon::proto as dpb;
use rill_daemon::ChangeKind;
use rill_types::RepoPathBuf;
use serde_json::json as j;

use super::args::ResolveArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// Run `rill conflicts`: the checkout's conflicted paths.
pub async fn conflicts(ctx: &mut Ctx<'_>) -> Result<()> {
    let conflicted = conflicted(ctx).await?;
    ctx.emit(
        j!({ "conflicts": conflicted.iter().map(|p| p.as_str()).collect::<Vec<_>>() }),
        |out| {
            if conflicted.is_empty() {
                return out.line(out.dim("no conflicted paths"));
            }
            for p in &conflicted {
                out.line(format!("{} {p}", out.bad("C")))?;
            }
            out.line(
                out.dim("edit the markers, then `rill resolve <paths>` or `rill resolve --all`"),
            )
        },
    )
}

async fn conflicted(ctx: &mut Ctx<'_>) -> Result<Vec<RepoPathBuf>> {
    let mount = ctx.mount().await?;
    let reply = ctx
        .daemon()
        .await?
        .get_status(dpb::GetStatusRequest {
            name: mount.name,
            prefixes: vec![],
        })
        .await?;
    Ok(rill_daemon::client::local_changes(&reply.changes)
        .into_iter()
        .filter(|c| c.kind == ChangeKind::Conflict)
        .map(|c| c.path)
        .collect())
}

/// Run `rill resolve`.
pub async fn resolve(ctx: &mut Ctx<'_>, args: ResolveArgs) -> Result<()> {
    if args.interactive {
        let paths: Vec<RepoPathBuf> = if args.paths.is_empty() {
            conflicted(ctx).await?
        } else {
            args.paths
                .iter()
                .map(RepoPathBuf::new)
                .collect::<std::result::Result<_, _>>()?
        };
        if paths.is_empty() {
            return ctx.emit(j!({ "resolved": [], "remaining": [] }), |out| {
                out.line(out.dim("no conflicted paths"))
            });
        }
        return super::pick::interactive(ctx, paths).await;
    }
    if args.paths.is_empty() && !args.all {
        return Err(CliError::usage(
            "name the paths to mark resolved, or `--all`; `rill conflicts` lists them",
        ));
    }
    let conflicted = conflicted(ctx).await?;
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
    let mount = ctx.mount().await?;
    ctx.daemon()
        .await?
        .resolve_paths(dpb::ResolvePathsRequest {
            name: mount.name,
            paths: paths.iter().map(|p| p.as_str().to_string()).collect(),
            force: args.force,
        })
        .await?;
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
