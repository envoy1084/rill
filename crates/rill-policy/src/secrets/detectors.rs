//! What a credential looks like: known token formats, which almost never
//! match anything else, and a guarded entropy check for the rest.

use std::sync::LazyLock;

use regex::Regex;

/// One kind of credential. The regex's first capture group (or the whole
/// match) is the secret; `min_entropy` rejects matches that look like
/// placeholders.
pub struct Detector {
    pub name: &'static str,
    pub regex: Regex,
    pub min_entropy: f64,
}

fn detector(name: &'static str, pattern: &str, min_entropy: f64) -> Detector {
    Detector {
        name,
        regex: Regex::new(pattern).expect("built-in secret patterns compile"),
        min_entropy,
    }
}

/// The built-in detectors.
pub static BUILT_IN: LazyLock<Vec<Detector>> = LazyLock::new(|| {
    vec![
        detector("aws-access-key-id", r"\b((?:AKIA|ASIA)[0-9A-Z]{16})\b", 3.0),
        detector(
            "aws-secret-access-key",
            r#"(?i)aws.{0,20}(?:secret|sk).{0,20}['"=:\s]([0-9A-Za-z/+]{40})\b"#,
            4.0,
        ),
        detector("github-token", r"\b(gh[pousr]_[A-Za-z0-9]{36,255})\b", 3.5),
        detector(
            "github-fine-grained-token",
            r"\b(github_pat_[A-Za-z0-9_]{60,255})\b",
            3.5,
        ),
        detector("slack-token", r"\b(xox[baprs]-[A-Za-z0-9-]{10,})\b", 3.0),
        detector("stripe-key", r"\b((?:sk|rk)_live_[0-9a-zA-Z]{24,})\b", 3.5),
        detector("google-api-key", r"\b(AIza[0-9A-Za-z_-]{35})\b", 3.5),
        detector(
            "gcp-service-account",
            r#""private_key"\s*:\s*"(-----BEGIN PRIVATE KEY-----[^"]{40,})"#,
            0.0,
        ),
        detector(
            "private-key",
            r"(-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY(?: BLOCK)?-----)",
            0.0,
        ),
        detector(
            "jwt",
            r"\b(eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{16,})\b",
            3.5,
        ),
        detector(
            "rill-refresh-token",
            r"\b(ss_[0-9a-f]{32}\.[A-Za-z0-9_-]{20,})\b",
            3.5,
        ),
        detector(
            "rill-scim-credential",
            r"\b(scim_[0-9a-f]{32}\.[A-Za-z0-9_-]{20,})\b",
            3.5,
        ),
        detector(
            "generic-secret",
            r#"(?i)(?:api[_-]?key|secret|token|passw(?:or)?d|credential)s?["']?\s*[:=]\s*["']([A-Za-z0-9/+_\-.=]{20,})["']"#,
            4.0,
        ),
    ]
});

/// Shannon entropy in bits per character: random keys score above 4,
/// words and placeholders well below.
pub fn entropy(s: &str) -> f64 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let len = bytes.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_strings_score_high_and_placeholders_low() {
        assert!(entropy("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY") > 4.0);
        assert!(entropy("xxxxxxxxxxxxxxxxxxxxxxxx") < 1.0);
        assert!(entropy("your-api-key-goes-here-please") < 4.0);
    }
}
