//! The reviewer fleet.

use rill_sdk::{ReviewRun, ReviewerStats};
use serde_json::{json, Value};

use super::{hash, timestamp};

/// A reviewer's run.
pub fn review_run(r: &ReviewRun) -> Value {
    json!({
        "reviewer": r.reviewer,
        "kind": r.kind,
        "version": hash(&r.version),
        "reason": r.reason,
        "status": r.status.as_str(),
        "queued_at": timestamp(&r.queued_at),
        "finished_at": r.finished_at.as_ref().map(timestamp),
        "findings": r.findings,
        "vote": r.vote,
        "model": r.model,
        "detail": r.detail,
    })
}

/// A reviewer's track record.
pub fn reviewer_stats(s: &ReviewerStats) -> Value {
    json!({
        "reviewer": s.reviewer,
        "trust": s.trust,
        "outcomes": s.outcomes,
        "approvals": s.approvals,
        "escapes": s.escapes,
        "blocks": s.blocks,
        "dismissed": s.dismissed,
        "demoted": s.demoted,
        "note": s.note,
        "since": s.since.as_ref().map(timestamp),
    })
}
