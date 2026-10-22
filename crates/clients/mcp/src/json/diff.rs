//! Diff results.

use rill_sdk::{DiffResult, FileDiff, Hunk};
use serde_json::{json, Value};

use super::history::{file_state, merge_terms};
use super::primitives::enum_name;

/// A structured hunk with its lines.
pub fn hunk(h: &Hunk) -> Value {
    json!({
        "before_start": h.before_start,
        "before_len": h.before_len,
        "after_start": h.after_start,
        "after_len": h.after_len,
        "lines": h.lines.iter().map(|l| json!({
            "op": enum_name(&l.op),
            "text": String::from_utf8_lossy(&l.text),
        })).collect::<Vec<_>>(),
    })
}

/// One path's diff, with a `status` of `added`, `deleted`, `modified` or
/// `conflict`.
pub fn file_diff(d: &FileDiff) -> Value {
    let status = if d.conflict.is_some() {
        "conflict"
    } else if d.is_added() {
        "added"
    } else if d.is_deleted() {
        "deleted"
    } else {
        "modified"
    };
    json!({
        "path": d.path.as_str(),
        "status": status,
        "before": file_state(&d.before),
        "after": file_state(&d.after),
        "binary": d.binary,
        "hunks": d.hunks.iter().map(hunk).collect::<Vec<_>>(),
        "unified": d.unified,
        "conflict": d.conflict.as_ref().map(merge_terms),
    })
}

/// A diff response.
pub fn diff_result(r: &DiffResult) -> Value {
    json!({
        "files": r.files.iter().map(file_diff).collect::<Vec<_>>(),
        "truncated": r.truncated,
    })
}
