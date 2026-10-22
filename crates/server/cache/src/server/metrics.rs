//! The cache's counters in OpenMetrics text.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};

use super::Cache;

pub(super) async fn metrics(State(cache): State<Arc<Cache>>) -> Response {
    let s = &cache.stats;
    let mut out = String::new();
    for (name, help, value) in [
        (
            "rill_cache_hits",
            "Objects served from disk.",
            s.hits.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_misses",
            "Objects not on disk.",
            s.misses.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_coalesced",
            "Misses served by another request's fetch.",
            s.coalesced.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_origin_fetches",
            "Fetches from the origin.",
            s.origin_fetches.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_denied",
            "Requests refused for want of a grant.",
            s.denied.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_evictions",
            "Objects evicted from disk.",
            cache.disk.evictions(),
        ),
    ] {
        out.push_str(&format!(
            "# HELP {name} {help}\n# TYPE {name} counter\n{name}_total {value}\n"
        ));
    }
    out.push_str(&format!(
        "# HELP rill_cache_bytes Bytes on disk.\n# TYPE rill_cache_bytes gauge\nrill_cache_bytes {}\n# EOF\n",
        cache.disk.bytes()
    ));
    (
        [(
            header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        out,
    )
        .into_response()
}
