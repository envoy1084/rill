//! Code search results.

use std::collections::BTreeSet;

use rill_sdk::{GrepHit, GrepResult, References, SymbolLocation};
use serde_json::{json, Value};

/// One matching line, with its context when the query asked for some.
pub fn grep_hit(h: &GrepHit) -> Value {
    let mut out = json!({
        "path": h.path.as_str(),
        "line_no": h.line_no,
        "line": h.line,
    });
    if !h.before.is_empty() || !h.after.is_empty() {
        out["before"] = json!(h.before);
        out["after"] = json!(h.after);
    }
    out
}

/// A search result: hits, how many files they span, and whether the
/// server cut the list short.
pub fn grep_result(r: &GrepResult) -> Value {
    let files: BTreeSet<&str> = r.hits.iter().map(|h| h.path.as_str()).collect();
    json!({
        "matches": r.hits.iter().map(grep_hit).collect::<Vec<_>>(),
        "files_searched": files.len(),
        "truncated": r.truncated,
    })
}

/// A symbol definition or use.
pub fn symbol_location(s: &SymbolLocation) -> Value {
    let mut out = json!({
        "path": s.path.as_str(),
        "line": s.line,
        "name": s.name,
    });
    if !s.kind.is_empty() {
        out["kind"] = json!(s.kind);
    }
    if s.precise {
        out["precise"] = json!(true);
        out["column"] = json!(s.start_column);
        out["symbol"] = json!(s.symbol);
    }
    if s.definition {
        out["definition"] = json!(true);
    }
    out
}

/// A reference list: precise (SCIP) or whole-word text matches.
pub fn references(r: &References) -> Value {
    json!({
        "references": r.references.iter().map(symbol_location).collect::<Vec<_>>(),
        "precise": r.precise,
        "truncated": r.truncated,
    })
}
