//! Trunk status, landings, verification, reverts and bisects.

use rill_sdk::{
    BisectResult, Landing, RevertResult, TrunkStatus, VerificationRecord, VerificationReport,
};
use serde_json::{json, Value};

use super::history::{change_version, land_event};
use super::primitives::{enum_name, hash, opt, principal, timestamp};
use super::review::land_ticket;

/// One runner's report.
pub fn verification_report(r: &VerificationReport) -> Value {
    json!({
        "target": r.target,
        "status": enum_name(&r.status),
        "reporter": principal(&r.reporter),
        "reported_at": timestamp(&r.reported_at),
        "details": r.details,
        "artifact": opt(r.artifact.as_ref(), hash),
    })
}

/// Aggregate verification of one sequence.
pub fn verification_record(r: &VerificationRecord) -> Value {
    json!({
        "seq": r.seq.get(),
        "status": enum_name(&r.status),
        "culprit": r.culprit.map(|s| s.get()),
        "reports": r.reports.iter().map(verification_report).collect::<Vec<_>>(),
    })
}

/// A trunk snapshot.
pub fn trunk_status(t: &TrunkStatus) -> Value {
    json!({
        "head_seq": t.head_seq.get(),
        "head_hash": opt(t.head_hash.as_ref(), hash),
        "in_flight": t.in_flight.iter().map(land_ticket).collect::<Vec<_>>(),
        "recent": t.recent.iter().map(land_event).collect::<Vec<_>>(),
        "last_verified_seq": t.last_verified_seq.map(|s| s.get()),
    })
}

/// A landing with its verification.
pub fn landing(l: &Landing) -> Value {
    json!({
        "event": land_event(&l.event),
        "verification": l.verification.as_ref().map(verification_record),
    })
}

/// A revert landing.
pub fn revert_result(r: &RevertResult) -> Value {
    json!({ "landing": land_event(&r.landing), "version": change_version(&r.version) })
}

/// A bisect outcome.
pub fn bisect_result(b: &BisectResult) -> Value {
    json!({
        "culprit": b.culprit.map(|s| s.get()),
        "tested": b.tested.iter().map(verification_record).collect::<Vec<_>>(),
    })
}
