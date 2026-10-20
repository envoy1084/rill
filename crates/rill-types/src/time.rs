use std::fmt;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Milliseconds since the Unix epoch, UTC.
///
/// Stored as an integer so that encoding is canonical and comparison is
/// trivial; formatted as RFC 3339 for humans.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    pub fn now() -> Self {
        let now = OffsetDateTime::now_utc();
        Timestamp((now.unix_timestamp_nanos() / 1_000_000) as i64)
    }

    pub const fn from_millis(ms: i64) -> Self {
        Timestamp(ms)
    }

    pub const fn millis(&self) -> i64 {
        self.0
    }

    pub fn from_unix_seconds(secs: i64) -> Self {
        Timestamp(secs * 1000)
    }

    pub fn to_rfc3339(&self) -> String {
        OffsetDateTime::from_unix_timestamp_nanos(self.0 as i128 * 1_000_000)
            .ok()
            .and_then(|t| t.format(&Rfc3339).ok())
            .unwrap_or_else(|| self.0.to_string())
    }

    pub fn parse_rfc3339(s: &str) -> Option<Self> {
        let t = OffsetDateTime::parse(s, &Rfc3339).ok()?;
        Some(Timestamp((t.unix_timestamp_nanos() / 1_000_000) as i64))
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}
