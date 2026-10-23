//! Short builders for paths and timestamps.

use rill_types::{RepoPathBuf, Timestamp};

pub fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

pub fn t(ms: i64) -> Timestamp {
    Timestamp::from_millis(ms)
}
