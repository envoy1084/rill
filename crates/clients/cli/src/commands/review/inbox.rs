//! `rill inbox`: reviews waiting on you, and the state of your own.

use rill_sdk::ReviewFilter;
use rill_types::{Principal, ReviewState, ReviewStatus};
use serde_json::json as j;

use crate::context::Ctx;
use crate::output::{ago, Out, Role};
use crate::Result;

/// Reviews looked at.
const LIMIT: u32 = 200;

/// Whether `review` waits on `me`: a thread addressed to me is open, or
/// someone else's review has no Review vote from me yet.
pub fn waits_on(review: &ReviewState, me: &Principal) -> bool {
    if review.owner == *me {
        return false;
    }
    let addressed = review
        .threads
        .iter()
        .any(|t| !t.resolved && t.to.as_ref() == Some(me));
    let voted = review
        .votes
        .iter()
        .any(|v| v.label == "Review" && v.voter == *me);
    addressed || !voted
}

fn approvals(r: &ReviewState) -> usize {
    r.votes
        .iter()
        .filter(|v| v.label == "Review" && v.value > 0)
        .count()
}

fn open_threads(r: &ReviewState) -> usize {
    r.threads.iter().filter(|t| !t.resolved).count()
}

fn line(out: &Out<'_>, r: &ReviewState) -> String {
    let status = match r.status {
        ReviewStatus::Landable => out.paint(Role::Good, "landable"),
        _ => out.paint(Role::Warning, "open"),
    };
    format!(
        "  {}  {status}  {} approval{} · {} open thread{} · {} · {}",
        out.paint(Role::Id, &r.change_id.to_string()[..11]),
        approvals(r),
        if approvals(r) == 1 { "" } else { "s" },
        open_threads(r),
        if open_threads(r) == 1 { "" } else { "s" },
        out.paint(Role::Author, &r.owner.to_string()),
        out.paint(Role::Date, &ago(&r.updated_at)),
    )
}

fn item_line(out: &Out<'_>, i: &rill_sdk::pb::InboxItem) -> String {
    let kind = match i.kind.as_known() {
        Some(rill_sdk::pb::InboxKind::Escalation) => out.paint(Role::Warning, "escalated"),
        Some(rill_sdk::pb::InboxKind::Audit) => out.paint(Role::Warning, "audit"),
        Some(rill_sdk::pb::InboxKind::OwnerReview) => "owner review".to_string(),
        Some(rill_sdk::pb::InboxKind::IntentApproval) => "intent".to_string(),
        _ => "thread".to_string(),
    };
    let what = [&i.change, &i.intent]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(|s| s.rsplit('/').next().unwrap_or_default().to_string())
        .unwrap_or_default();
    format!(
        "  {kind:<12} {}  {}{}",
        out.paint(Role::Id, &what),
        i.title,
        if i.detail.is_empty() {
            String::new()
        } else {
            format!(" ({})", i.detail)
        }
    )
}

/// Run `rill inbox`.
pub async fn inbox(ctx: &mut Ctx<'_>) -> Result<()> {
    let me = ctx.client()?.admin().whoami().await?.principal;
    let needs = ctx.repo()?.attention().inbox(50).await?;
    let reviews = ctx
        .repo()?
        .reviews()
        .list(ReviewFilter {
            status: vec![ReviewStatus::Open, ReviewStatus::Landable],
            limit: LIMIT,
            ..ReviewFilter::default()
        })
        .await?
        .items;
    let (yours, others): (Vec<&ReviewState>, Vec<&ReviewState>) =
        reviews.iter().partition(|r| r.owner == me);
    let waiting: Vec<&ReviewState> = others.into_iter().filter(|r| waits_on(r, &me)).collect();
    let summary = |r: &ReviewState| {
        j!({
            "change": r.change_id.to_string(),
            "owner": r.owner.to_string(),
            "status": rill_mcp::json::enum_name(&r.status),
            "approvals": approvals(r),
            "open_threads": open_threads(r),
        })
    };
    let value = j!({
        "needs_you": needs.iter().map(|i| j!({
            "kind": i.kind.to_i32(), "change": i.change, "intent": i.intent, "landing": i.landing,
            "title": i.title, "detail": i.detail, "urgency": i.urgency,
        })).collect::<Vec<_>>(),
        "waiting": waiting.iter().map(|r| summary(r)).collect::<Vec<_>>(),
        "yours": yours.iter().map(|r| summary(r)).collect::<Vec<_>>(),
    });
    ctx.emit(value, |out| {
        let header = out.paint(Role::Header, "Needs you");
        out.line(header)?;
        if needs.is_empty() {
            out.line(out.dim("  nothing"))?;
        }
        for i in &needs {
            out.line(item_line(out, i))?;
        }
        out.blank()?;
        let header = out.paint(Role::Header, "Not yet reviewed by you");
        out.line(header)?;
        if waiting.is_empty() {
            out.line(out.dim("  nothing"))?;
        }
        for r in &waiting {
            out.line(line(out, r))?;
        }
        out.blank()?;
        let header = out.paint(Role::Header, "Your changes");
        out.line(header)?;
        if yours.is_empty() {
            out.line(out.dim("  none open"))?;
        }
        for r in &yours {
            out.line(line(out, r))?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ChangeId, LabelVote, RiskClass, Thread, ThreadAnchor, Timestamp};

    fn review(owner: Principal) -> ReviewState {
        ReviewState {
            change_id: ChangeId::new(),
            versions: vec![],
            status: ReviewStatus::Open,
            intent: None,
            owner,
            votes: vec![],
            threads: vec![],
            risk: RiskClass::Low,
            risk_reasons: vec![],
            stack: vec![],
            landed_seq: None,
            created_at: Timestamp::from_millis(0),
            updated_at: Timestamp::from_millis(0),
            extra: Default::default(),
        }
    }

    #[test]
    fn unvoted_reviews_and_threads_addressed_to_me_wait_on_me() {
        let (me, ann) = (Principal::human("me"), Principal::human("ann"));
        assert!(!waits_on(&review(me.clone()), &me), "my own review");
        let mut r = review(ann.clone());
        assert!(waits_on(&r, &me), "no vote from me yet");
        r.votes.push(LabelVote {
            label: "Review".into(),
            value: 2,
            voter: me.clone(),
            version: rill_types::ObjectHash::from_bytes([0; 32]),
            voted_at: Timestamp::from_millis(0),
            reason: None,
        });
        assert!(!waits_on(&r, &me));
        r.threads.push(Thread {
            id: "t".into(),
            anchor: ThreadAnchor::Change,
            comments: vec![],
            resolved: false,
            to: Some(me.clone()),
            outdated: false,
        });
        assert!(waits_on(&r, &me), "a thread addressed to me");
    }
}
