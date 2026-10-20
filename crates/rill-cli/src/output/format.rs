//! Small formatting helpers for human output.

use rill_types::{ObjectHash, Timestamp};

/// The first 12 hex characters of a hash.
pub fn short(hash: &ObjectHash) -> String {
    hash.to_hex()[..12].to_string()
}

/// A timestamp as RFC 3339 without sub-second noise.
pub fn when(t: &Timestamp) -> String {
    let text = t.to_rfc3339();
    match text.split_once('.') {
        Some((head, tail)) => {
            // Keep the zone suffix (`Z` or `+hh:mm`) after the fraction.
            let zone = tail.trim_start_matches(|c: char| c.is_ascii_digit());
            format!("{head}{zone}")
        }
        None => text,
    }
}

/// `yes` / `no`.
pub fn yes_no(v: bool) -> &'static str {
    if v {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_fractional_seconds() {
        let t = Timestamp::from_millis(1_700_000_000_123);
        assert_eq!(when(&t), "2023-11-14T22:13:20Z");
        let whole = Timestamp::from_millis(1_700_000_000_000);
        assert_eq!(when(&whole), "2023-11-14T22:13:20Z");
    }
}
