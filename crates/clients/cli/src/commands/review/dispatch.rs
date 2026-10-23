//! Routing `rill review` subcommands.

use super::args::ReviewCmd;
use super::lifecycle::{evaluate, list, show, vote};
use super::threads::{comment, reply, resolve_thread, threads};
use crate::context::Ctx;
use crate::Result;

/// Run a review subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ReviewCmd) -> Result<()> {
    match cmd {
        ReviewCmd::Open(args) => {
            let repo = ctx.repo()?;
            let id = super::working_change::change_or_working(ctx, args.change.as_deref(), &repo)
                .await?;
            super::browse::open(ctx, id).await
        }
        ReviewCmd::Show(args) => show(ctx, args).await,
        ReviewCmd::List(args) => list(ctx, args).await,
        ReviewCmd::Vote(args) => vote(ctx, args).await,
        ReviewCmd::Comment(args) => comment(ctx, args).await,
        ReviewCmd::Reply(args) => reply(ctx, args).await,
        ReviewCmd::Threads(args) => threads(ctx, args).await,
        ReviewCmd::ResolveThread(args) => resolve_thread(ctx, args).await,
        ReviewCmd::Evaluate(args) => evaluate(ctx, args).await,
        ReviewCmd::Request(args) => super::fleet::request(ctx, args).await,
        ReviewCmd::Runs(args) => super::fleet::runs(ctx, args).await,
        ReviewCmd::Audits => super::attention::audits(ctx).await,
        ReviewCmd::Audit(args) => super::attention::audit(ctx, args).await,
        ReviewCmd::Batch(args) => super::attention::batch(ctx, args).await,
        ReviewCmd::Interdiff(args) => super::mechanics::interdiff(ctx, args).await,
        ReviewCmd::Apply(args) => super::mechanics::apply(ctx, args).await,
        ReviewCmd::Reopen(args) => super::mechanics::reopen(ctx, args).await,
    }
}
