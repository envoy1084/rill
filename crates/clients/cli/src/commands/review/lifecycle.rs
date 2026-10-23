//! Submitting, inspecting, voting on, evaluating and abandoning reviews.

use rill_mcp::json;
use rill_sdk::ReviewFilter;
use rill_types::ReviewStatus;

use super::args::{AbandonArgs, ApproveArgs, ChangeArg, ListArgs, SubmitArgs, VoteArgs};
use super::render::{print_state, status_text};
use super::working_change::change_or_working;
use crate::commands::change_ref;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{short, when, Table};
use crate::Result;

fn parse_status(s: &str) -> Result<ReviewStatus> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "open" => ReviewStatus::Open,
        "landable" => ReviewStatus::Landable,
        "landing" => ReviewStatus::Landing,
        "landed" => ReviewStatus::Landed,
        "abandoned" => ReviewStatus::Abandoned,
        other => return Err(CliError::usage(format!("unknown review status {other:?}"))),
    })
}

/// Run `rill submit`.
pub async fn submit(ctx: &mut Ctx<'_>, args: SubmitArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let state = repo.reviews().submit(id).await?;
    ctx.emit(json::review_state(&state), |out| {
        out.line(format!(
            "submitted {} for review ({})",
            out.id(&id.to_string()),
            status_text(out, state.status)
        ))
    })
}

pub(super) async fn show(ctx: &mut Ctx<'_>, args: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let info = repo.reviews().get(id).await?;
    ctx.emit(json::review_info(&info), |out| {
        print_state(out, &info.review)
    })
}

pub(super) async fn list(ctx: &mut Ctx<'_>, args: ListArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let filter = ReviewFilter {
        status: args
            .status
            .as_deref()
            .map(parse_status)
            .transpose()?
            .into_iter()
            .collect(),
        owner: args.owner.clone(),
        intent: None,
        limit: args.limit,
        page_token: None,
    };
    let page = repo.reviews().list(filter).await?;
    ctx.emit(json::page(&page, json::review_state), |out| {
        if page.items.is_empty() {
            return out.line(out.dim("no reviews"));
        }
        let mut table = Table::new(&["CHANGE", "STATUS", "RISK", "OWNER", "VERSION", "UPDATED"]);
        for r in &page.items {
            table.row(vec![
                out.id(&r.change_id.to_string()),
                status_text(out, r.status),
                r.risk.to_string(),
                r.owner.to_string(),
                r.current_version().map(|h| short(&h)).unwrap_or_default(),
                when(&r.updated_at),
            ]);
        }
        out.table(table)
    })
}

pub(super) async fn vote(ctx: &mut Ctx<'_>, args: VoteArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let value: i8 = args
        .value
        .trim_start_matches('+')
        .parse()
        .map_err(|_| CliError::usage(format!("invalid vote value {:?}", args.value)))?;
    let state = repo
        .reviews()
        .vote(id, &args.label, value, args.message, None)
        .await?;
    ctx.emit(json::review_state(&state), |out| {
        out.line(format!(
            "voted {} {:+} on {} ({})",
            args.label,
            value,
            out.id(&id.to_string()),
            status_text(out, state.status)
        ))
    })
}

pub(super) async fn evaluate(ctx: &mut Ctx<'_>, args: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let decision = repo.reviews().evaluate_submit(id).await?;
    let allowed = decision.allowed;
    ctx.emit(json::submit_decision(&decision), |out| {
        out.line(format!(
            "{}: {} (risk {})",
            out.id(&id.to_string()),
            if decision.allowed {
                out.good("landable")
            } else {
                out.bad("not landable")
            },
            decision.risk
        ))?;
        for r in &decision.reasons {
            out.line(format!("  - {r}"))?;
        }
        if !decision.matched_policies.is_empty() {
            out.line(out.dim(&format!(
                "policies: {}",
                decision.matched_policies.join(", ")
            )))?;
        }
        for r in &decision.risk_reasons {
            out.line(out.dim(&format!("risk: {r}")))?;
        }
        Ok(())
    })?;
    if allowed {
        Ok(())
    } else {
        Err(CliError::conflict("change is not landable"))
    }
}

/// Run `rill approve` (`reject: false`) or `rill reject`: a vote at the
/// label's most (or least), unless `--value` says.
pub async fn approve(ctx: &mut Ctx<'_>, args: ApproveArgs, reject: bool) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let reviews = repo.reviews();
    let value = match args.value {
        Some(v) => v,
        None => {
            let info = reviews.get(id).await?;
            let label = info
                .labels
                .iter()
                .find(|l| l.name == args.label)
                .ok_or_else(|| CliError::usage(format!("no label {:?} here", args.label)))?;
            if reject {
                label.range.min
            } else {
                label.range.max
            }
        }
    };
    let state = reviews
        .vote(id, &args.label, value, args.message, None)
        .await?;
    ctx.emit(json::review_state(&state), |out| {
        out.line(format!(
            "voted {} {:+} on {} ({})",
            args.label,
            value,
            out.id(&id.to_string()),
            status_text(out, state.status)
        ))
    })
}

/// Run `rill abandon`.
pub async fn abandon(ctx: &mut Ctx<'_>, args: AbandonArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let state = repo.reviews().abandon(id, args.message).await?;
    ctx.emit(json::review_state(&state), |out| {
        out.line(format!("abandoned {}", out.id(&id.to_string())))
    })
}
