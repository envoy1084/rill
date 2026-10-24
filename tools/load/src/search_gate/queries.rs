//! The query mix: what developers and agents type into code search.

use rand::rngs::StdRng;
use rand::RngExt;
use rill_search::SearchQuery;

use super::corpus::word;

/// Hits a query asks for, as a search page does.
const PAGE: usize = 50;

/// A query of the mix and its kind, for reporting.
pub fn next(rng: &mut StdRng) -> (&'static str, SearchQuery) {
    let roll = rng.random_range(0..100);
    let w = |rng: &mut StdRng, lo: usize, hi: usize| word(rng.random_range(lo..hi));
    let (kind, q) = match roll {
        0..40 => (
            "identifier",
            SearchQuery::new(w(rng, 200, 5_000)).literal(true),
        ),
        40..60 => (
            "rare",
            SearchQuery::new(w(rng, 20_000, 50_000)).literal(true),
        ),
        60..70 => ("common", SearchQuery::new(w(rng, 0, 50)).literal(true)),
        70..80 => {
            let name = w(rng, 100, 20_000);
            (
                "regex",
                SearchQuery::new(format!(r"(fn|def|func|function) {name}\(")),
            )
        }
        80..85 => {
            let prefix = &w(rng, 0, 5_000)[..4];
            ("prefix", SearchQuery::new(format!(r"\b{prefix}\w*_\w+\(")))
        }
        85..90 => (
            "missing",
            SearchQuery::new(format!("zq{}x", rng.random_range(0..1_000_000))),
        ),
        90..95 => (
            "language",
            SearchQuery::new(w(rng, 200, 5_000)).languages(vec!["python".into()]),
        ),
        _ => (
            "case",
            SearchQuery::new(w(rng, 200, 5_000).to_uppercase()).case_insensitive(true),
        ),
    };
    (kind, q.max_results(PAGE))
}
