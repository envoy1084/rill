//! `rill review request` and `rill review runs`: agent reviews of a change.

use rill_mcp::json;

use super::args::{ChangeArg, RequestArgs};
use super::working_change::change_or_working;
use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

fn table(out: &crate::output::Out<'_>, runs: &[rill_sdk::ReviewRun]) -> Table {
    let mut table = Table::new(&[
        "REVIEWER", "STATUS", "VERSION", "VOTE", "FINDINGS", "DETAIL",
    ]);
    for r in runs {
        let status = match r.status {
            rill_sdk::ReviewRunStatus::Done => out.good("done"),
            rill_sdk::ReviewRunStatus::Failed => out.warn("failed"),
            s => s.as_str().to_string(),
        };
        table.row(vec![
            out.bold(&r.reviewer),
            status,
            r.version.to_hex()[..12].to_string(),
            if r.vote == 0 {
                String::new()
            } else {
                format!("{:+}", r.vote)
            },
            r.findings.to_string(),
            r.detail.clone(),
        ]);
    }
    table
}

/// Queue agent reviews of a change's current version.
pub async fn request(ctx: &mut Ctx<'_>, args: RequestArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let runs = repo.reviewers().request(id, &args.reviewers).await?;
    ctx.emit(
        serde_json::json!(runs.iter().map(json::review_run).collect::<Vec<_>>()),
        |out| out.table(table(out, &runs)),
    )
}

/// A change's agent review runs, newest first.
pub async fn runs(ctx: &mut Ctx<'_>, args: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let runs = repo.reviewers().runs(id).await?;
    ctx.emit(
        serde_json::json!(runs.iter().map(json::review_run).collect::<Vec<_>>()),
        |out| {
            if runs.is_empty() {
                return out.line(out.dim("no agent reviews"));
            }
            out.table(table(out, &runs))
        },
    )
}
