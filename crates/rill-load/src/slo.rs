//! The service-level objectives a run is judged against (research plan,
//! target workload), measured at the client.

use serde_json::{json, Value};

/// `(operation, quantile, milliseconds)`.
const OBJECTIVES: &[(&str, f64, f64)] = &[
    ("read_file", 0.5, 10.0),
    ("read_file", 0.99, 50.0),
    ("list_dir", 0.5, 10.0),
    ("list_dir", 0.99, 50.0),
    ("log", 0.5, 10.0),
    ("log", 0.99, 50.0),
    ("create_version", 0.99, 300.0),
    ("grep", 0.99, 1_000.0),
    ("land", 0.5, 15_000.0),
    ("land", 0.99, 120_000.0),
    ("watch_event", 0.99, 10_000.0),
];

/// Each objective against the report's latencies; `met` is false when any
/// objective with data is missed.
pub fn judge(report: &Value) -> Value {
    let rows = report["ops"].as_array().cloned().unwrap_or_default();
    let mut met = true;
    let results: Vec<Value> = OBJECTIVES
        .iter()
        .filter_map(|(op, q, target)| {
            let row = rows.iter().find(|r| r["op"] == *op)?;
            let field = if *q == 0.5 { "p50_ms" } else { "p99_ms" };
            let actual = row[field].as_f64()?;
            let pass = actual <= *target;
            met &= pass;
            Some(json!({ "op": op, "quantile": q, "target_ms": target, "actual_ms": actual, "pass": pass }))
        })
        .collect();
    json!({ "met": met, "objectives": results })
}
