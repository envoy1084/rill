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

/// How long before `now` a timestamp was, the way git says it: `just
/// now`, `5 minutes ago`, `3 days ago`, `2 months ago`, `in 1 hour`.
pub fn ago_from(t: &Timestamp, now: &Timestamp) -> String {
    let diff = now.millis() - t.millis();
    let (secs, future) = if diff < 0 {
        ((-diff) / 1000, true)
    } else {
        (diff / 1000, false)
    };
    let (n, unit) = match secs {
        s if s < 45 => return "just now".into(),
        s if s < 3570 => ((s + 30) / 60, "minute"),
        s if s < 36 * 3600 => ((s + 1800) / 3600, "hour"),
        s if s < 45 * 86_400 => ((s + 43_200) / 86_400, "day"),
        s if s < 365 * 86_400 => ((s + 15 * 86_400) / (30 * 86_400), "month"),
        s => ((s + 182 * 86_400) / (365 * 86_400), "year"),
    };
    let plural = if n == 1 { "" } else { "s" };
    if future {
        format!("in {n} {unit}{plural}")
    } else {
        format!("{n} {unit}{plural} ago")
    }
}

/// [`ago_from`] now.
pub fn ago(t: &Timestamp) -> String {
    ago_from(t, &Timestamp::now())
}

/// `s` without ANSI escape sequences: colours (CSI) and hyperlinks (OSC,
/// ended by BEL or `ESC \`).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            Some('[') => {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Columns `s` takes on screen (escape sequences take none).
pub fn visible_width(s: &str) -> usize {
    strip_ansi(s).chars().count()
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
    fn escapes_of_colours_and_links_are_stripped() {
        let link = "\u{1b}]8;;file:///a\u{1b}\\a.rs\u{1b}]8;;\u{1b}\\";
        assert_eq!(strip_ansi(&format!("\u{1b}[31m{link}\u{1b}[0m!")), "a.rs!");
        assert_eq!(visible_width("\u{1b}[1mab\u{1b}[0m"), 2);
    }

    #[test]
    fn relative_times_round_like_git() {
        let now = Timestamp::from_millis(1_700_000_000_000);
        let before = |secs: i64| Timestamp::from_millis(1_700_000_000_000 - secs * 1000);
        assert_eq!(ago_from(&before(10), &now), "just now");
        assert_eq!(ago_from(&before(60), &now), "1 minute ago");
        assert_eq!(ago_from(&before(5 * 60), &now), "5 minutes ago");
        assert_eq!(ago_from(&before(3 * 3600), &now), "3 hours ago");
        assert_eq!(ago_from(&before(2 * 86_400), &now), "2 days ago");
        assert_eq!(ago_from(&before(70 * 86_400), &now), "2 months ago");
        assert_eq!(ago_from(&before(800 * 86_400), &now), "2 years ago");
        assert_eq!(ago_from(&before(-3600), &now), "in 1 hour");
    }

    #[test]
    fn trims_fractional_seconds() {
        let t = Timestamp::from_millis(1_700_000_000_123);
        assert_eq!(when(&t), "2023-11-14T22:13:20Z");
        let whole = Timestamp::from_millis(1_700_000_000_000);
        assert_eq!(when(&whole), "2023-11-14T22:13:20Z");
    }
}
