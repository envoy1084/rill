//! Local checkout state (used by the CLI's `--json` output).

use rill_sdk::local::UpdateReport;
use rill_sdk::LocalChange;
use serde_json::{json, Value};

use super::primitives::enum_name;

/// One path that differs from the checkout's base.
pub fn local_change(c: &LocalChange) -> Value {
    json!({ "path": c.path.as_str(), "kind": enum_name(&c.kind) })
}

/// What a checkout update did.
pub fn update_report(r: &UpdateReport) -> Value {
    let paths = |v: &[rill_types::RepoPathBuf]| {
        v.iter().map(|p| p.as_str().to_string()).collect::<Vec<_>>()
    };
    json!({
        "updated": paths(&r.updated),
        "removed": paths(&r.removed),
        "merged": paths(&r.merged),
        "conflicts": paths(&r.conflicts),
    })
}
