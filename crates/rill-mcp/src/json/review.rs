//! Review state, votes, threads, submit decisions, land tickets and stack
//! results.

use rill_sdk::stack::{LandedChange, SyncResult};
use rill_sdk::{
    LandProgress, LandTicket, NewThread, ReviewEvent, ReviewEventKind, ReviewInfo, SubmitDecision,
};
use rill_types::{Comment, Label, LabelVote, ReviewState, Thread, ThreadAnchor};
use serde_json::{json, Value};

use super::primitives::{enum_name, hash, opt, principal, timestamp};

fn label(l: &Label) -> Value {
    json!({
        "name": l.name,
        "min": l.range.min,
        "max": l.range.max,
        "copy_on_trivial_rebase": l.copy_on_trivial_rebase,
        "max_requires_human": l.max_requires_human,
    })
}

fn vote(v: &LabelVote) -> Value {
    json!({
        "label": v.label,
        "value": v.value,
        "voter": principal(&v.voter),
        "version": hash(&v.version),
        "voted_at": timestamp(&v.voted_at),
        "reason": v.reason,
    })
}

fn anchor(a: &ThreadAnchor) -> Value {
    match a {
        ThreadAnchor::Change => json!({ "kind": "change" }),
        ThreadAnchor::Lines {
            version,
            path,
            start,
            end,
            context_hash,
        } => json!({
            "kind": "lines",
            "version": hash(version),
            "path": path.as_str(),
            "start": start,
            "end": end,
            "context_hash": context_hash,
        }),
        ThreadAnchor::File { version, path } => json!({
            "kind": "file",
            "version": hash(version),
            "path": path.as_str(),
        }),
    }
}

fn comment(c: &Comment) -> Value {
    json!({
        "author": principal(&c.author),
        "body": c.body,
        "created_at": timestamp(&c.created_at),
        "category": c.category.as_ref().map(enum_name),
        "confidence": c.confidence,
    })
}

/// A review thread with its comments.
pub fn thread(t: &Thread) -> Value {
    json!({
        "id": t.id,
        "anchor": anchor(&t.anchor),
        "comments": t.comments.iter().map(comment).collect::<Vec<_>>(),
        "resolved": t.resolved,
        "to": opt(t.to.as_ref(), principal),
    })
}

/// A review state.
pub fn review_state(r: &ReviewState) -> Value {
    json!({
        "change_id": r.change_id.to_string(),
        "versions": r.versions.iter().map(hash).collect::<Vec<_>>(),
        "current_version": opt(r.current_version().as_ref(), hash),
        "status": enum_name(&r.status),
        "intent": r.intent.map(|i| i.to_string()),
        "owner": principal(&r.owner),
        "votes": r.votes.iter().map(vote).collect::<Vec<_>>(),
        "threads": r.threads.iter().map(thread).collect::<Vec<_>>(),
        "unresolved_threads": r.unresolved_threads(),
        "risk": r.risk.as_str(),
        "risk_reasons": r.risk_reasons,
        "stack": r.stack.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
        "landed_seq": r.landed_seq.map(|s| s.get()),
        "created_at": timestamp(&r.created_at),
        "updated_at": timestamp(&r.updated_at),
        "extra": r.extra,
    })
}

/// A review with the repository's label definitions.
pub fn review_info(r: &ReviewInfo) -> Value {
    json!({
        "review": review_state(&r.review),
        "labels": r.labels.iter().map(label).collect::<Vec<_>>(),
    })
}

/// A submit evaluation.
pub fn submit_decision(d: &SubmitDecision) -> Value {
    json!({
        "allowed": d.allowed,
        "reasons": d.reasons,
        "matched_policies": d.matched_policies,
        "risk": d.risk.as_str(),
        "risk_reasons": d.risk_reasons,
    })
}

/// A land ticket.
pub fn land_ticket(t: &LandTicket) -> Value {
    json!({
        "id": t.id,
        "change_id": t.change_id.to_string(),
        "status": enum_name(&t.status),
        "requested_at": timestamp(&t.requested_at),
    })
}

/// A land ticket with its outcome.
pub fn land_progress(p: &LandProgress) -> Value {
    json!({
        "ticket": land_ticket(&p.ticket),
        "status": enum_name(&p.status()),
        "seq": p.seq.map(|s| s.get()),
        "landed_hash": opt(p.landed_hash.as_ref(), hash),
        "error": p.error,
    })
}

/// A new thread with its id.
pub fn new_thread(t: &NewThread) -> Value {
    json!({ "thread_id": t.thread_id, "thread": thread(&t.thread) })
}

/// A review event.
pub fn review_event(e: &ReviewEvent) -> Value {
    let (kind, data) = match &e.kind {
        ReviewEventKind::StatusChanged(s) => ("status_changed", enum_name(s)),
        ReviewEventKind::Voted(v) => ("voted", vote(v)),
        ReviewEventKind::ThreadUpdated(t) => ("thread_updated", thread(t)),
        ReviewEventKind::NewVersion(h) => ("new_version", hash(h)),
        ReviewEventKind::LandUpdate(t) => ("land_update", land_ticket(t)),
    };
    json!({
        "change_id": e.change_id.to_string(),
        "at": timestamp(&e.at),
        "kind": kind,
        "data": data,
    })
}

/// The result of syncing a stack.
pub fn sync_result(r: &SyncResult) -> Value {
    json!({
        "clean": r.is_clean(),
        "changes": r.changes.iter().map(|c| json!({
            "change_id": c.change_id.to_string(),
            "from": hash(&c.from),
            "to": hash(&c.to),
            "conflicts": c.conflicts.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// One change landed by a stack land.
pub fn landed_change(c: &LandedChange) -> Value {
    json!({
        "change_id": c.change_id.to_string(),
        "seq": c.seq.get(),
        "hash": hash(&c.hash),
    })
}
