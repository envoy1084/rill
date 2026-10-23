//! Loop detection. An agent that repeats the same failing call is stuck;
//! after a few identical failures the error says so and suggests a
//! different approach instead of letting it retry forever.

use std::hash::{DefaultHasher, Hash, Hasher};

use serde_json::Value;

/// Identical failures in a row before the error carries a stop hint.
pub const STOP_AFTER: u32 = 3;

/// The last failing call and how many times in a row it has failed.
#[derive(Debug, Default)]
pub struct RepeatGuard {
    last: Option<(u64, u32)>,
}

impl RepeatGuard {
    /// Record that the call `key` failed; how many times in a row it has.
    pub fn failed(&mut self, key: u64) -> u32 {
        let count = match self.last {
            Some((k, n)) if k == key => n + 1,
            _ => 1,
        };
        self.last = Some((key, count));
        count
    }

    /// Record a success, which breaks any streak.
    pub fn succeeded(&mut self) {
        self.last = None;
    }
}

/// Identity of a call: its tool and arguments.
pub fn call_key(name: &str, args: &Value) -> u64 {
    let mut h = DefaultHasher::new();
    name.hash(&mut h);
    args.to_string().hash(&mut h);
    h.finish()
}

/// The hint for a call that has failed `count` times in a row, once that
/// is enough to call it a loop.
pub fn stop_hint(count: u32) -> Option<String> {
    (count >= STOP_AFTER).then(|| {
        format!(
            "This exact call has failed {count} times in a row. Stop retrying it: read the error, \
             check state with rill_context or rill_review_status, and change the arguments or approach."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_failures_trip_the_guard_and_success_resets_it() {
        let mut g = RepeatGuard::default();
        let key = call_key("rill_land", &json!({ "change": "ch_1" }));
        let other = call_key("rill_land", &json!({ "change": "ch_2" }));
        assert_eq!(g.failed(key), 1);
        assert_eq!(g.failed(key), 2);
        assert!(stop_hint(2).is_none());
        assert_eq!(g.failed(key), 3);
        assert!(stop_hint(3).is_some());
        assert_eq!(g.failed(other), 1, "a different call starts a new streak");
        g.succeeded();
        assert_eq!(g.failed(other), 1);
    }
}
