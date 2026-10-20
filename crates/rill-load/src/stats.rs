//! Latency and error accounting per operation.

use std::collections::BTreeMap;
use std::time::Duration;

use hdrhistogram::Histogram;
use parking_lot::Mutex;
use serde_json::{json, Value};

#[derive(Default)]
struct OpStats {
    latency: Option<Histogram<u64>>,
    errors: u64,
    dropped: u64,
}

/// Shared counters for a run.
#[derive(Default)]
pub struct Stats {
    ops: Mutex<BTreeMap<&'static str, OpStats>>,
}

impl Stats {
    pub fn ok(&self, op: &'static str, took: Duration) {
        let mut ops = self.ops.lock();
        let entry = ops.entry(op).or_default();
        let hist = entry.latency.get_or_insert_with(|| {
            Histogram::new_with_bounds(1, 600_000_000, 3).expect("valid bounds")
        });
        let _ = hist.record(took.as_micros().clamp(1, 600_000_000) as u64);
    }

    pub fn error(&self, op: &'static str) {
        self.ops.lock().entry(op).or_default().errors += 1;
    }

    /// The operation could not start because too many were in flight.
    pub fn dropped(&self, op: &'static str) {
        self.ops.lock().entry(op).or_default().dropped += 1;
    }

    /// One row per operation: counts, achieved rate and latency
    /// percentiles in milliseconds.
    pub fn report(&self, elapsed: Duration) -> Value {
        let secs = elapsed.as_secs_f64().max(f64::EPSILON);
        let ops = self.ops.lock();
        let rows: Vec<Value> = ops
            .iter()
            .map(|(op, s)| {
                let count = s.latency.as_ref().map_or(0, Histogram::len);
                let ms = |q: f64| {
                    s.latency
                        .as_ref()
                        .map_or(0.0, |h| h.value_at_quantile(q) as f64 / 1000.0)
                };
                json!({
                    "op": op,
                    "ok": count,
                    "errors": s.errors,
                    "dropped": s.dropped,
                    "per_sec": count as f64 / secs,
                    "p50_ms": ms(0.5),
                    "p90_ms": ms(0.9),
                    "p99_ms": ms(0.99),
                    "max_ms": s.latency.as_ref().map_or(0.0, |h| h.max() as f64 / 1000.0),
                })
            })
            .collect();
        json!({ "elapsed_secs": secs, "ops": rows })
    }
}
