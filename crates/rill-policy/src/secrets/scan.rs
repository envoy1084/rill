//! Finding credentials in a file's content.

use super::detectors::{entropy, BUILT_IN};
use super::rules::SecretRules;

/// A credential found in content. Carries a fingerprint, never the
/// secret: findings are shown to people and stored in the audit log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    /// 1-based.
    pub line: usize,
    pub detector: String,
    pub fingerprint: String,
}

/// A stable, one-way name for a secret: the first 16 hex characters of
/// its BLAKE3 hash, for allow-listing a known fake.
pub fn fingerprint(secret: &str) -> String {
    hex::encode(&blake3::derive_key("rill 2026 secret fingerprint", secret.as_bytes())[..8])
}

/// Findings reported for one file; more are not looked for.
pub const MAX_FINDINGS: usize = 20;

/// The text to scan in `content`. Binary-looking content (a NUL in the
/// first 8 KiB) is scanned too: UTF-16LE is narrowed to its low bytes, and
/// other bytes outside printable ASCII become line breaks, leaving the
/// printable runs.
fn text_of(content: &[u8]) -> std::borrow::Cow<'_, str> {
    let head = &content[..content.len().min(8192)];
    if !head.contains(&0) {
        return String::from_utf8_lossy(content);
    }
    let odd_zeros = head.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    let bytes: Vec<u8> = if odd_zeros * 5 >= head.len() / 2 * 4 {
        content.iter().step_by(2).copied().collect()
    } else {
        content.to_vec()
    };
    let printable = bytes
        .into_iter()
        .map(|b| match b {
            b'\t' | b'\n' | b'\r' | 0x20..=0x7e => b,
            _ => b'\n',
        })
        .collect();
    std::borrow::Cow::Owned(String::from_utf8(printable).expect("printable ASCII"))
}

/// The credentials in `content` at `path` under `rules`, at most
/// [`MAX_FINDINGS`]. Allowed paths are skipped.
pub fn scan(path: &str, content: &[u8], rules: &SecretRules) -> Vec<Finding> {
    if rules.path_allowed(path) {
        return Vec::new();
    }
    let text = text_of(content);
    let mut seen = std::collections::HashSet::new();
    let mut hits: Vec<(usize, String, String)> = Vec::new();
    let mut add = |name: &str, start: usize, secret: &str| -> bool {
        let fp = fingerprint(secret);
        if !rules.allow_fingerprints.contains(&fp) && seen.insert(fp.clone()) {
            hits.push((start, name.to_string(), fp));
        }
        hits.len() < MAX_FINDINGS
    };
    let detectors = BUILT_IN
        .iter()
        .map(|d| (d.name, &d.regex, d.min_entropy))
        .chain(rules.patterns.iter().map(|(n, r)| (n.as_str(), r, 0.0)));
    'scan: for (name, regex, min_entropy) in detectors {
        for caps in regex.captures_iter(&text) {
            let m = caps
                .get(1)
                .or_else(|| caps.get(0))
                .expect("a match has group 0");
            if entropy(m.as_str()) >= min_entropy && !add(name, m.start(), m.as_str()) {
                break 'scan;
            }
        }
    }
    hits.sort_by_key(|(start, _, _)| *start);
    let (mut line, mut at) = (1, 0);
    hits.into_iter()
        .map(|(start, name, fingerprint)| {
            line += text.as_bytes()[at..start]
                .iter()
                .filter(|b| **b == b'\n')
                .count();
            at = start;
            Finding {
                path: path.to_string(),
                line,
                detector: name,
                fingerprint,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::ScanMode;

    fn found(path: &str, text: &str, rules: &SecretRules) -> Vec<String> {
        scan(path, text.as_bytes(), rules)
            .into_iter()
            .map(|f| format!("{}:{}", f.detector, f.line))
            .collect()
    }

    #[test]
    fn known_credentials_are_found_and_placeholders_are_not() {
        let rules = SecretRules::default();
        let leaked = [
            ("aws-access-key-id", "key = AKIAZ7T3KQ2MVXC4L9PW\n"),
            (
                "github-token",
                "token: ghp_Zr8Kq2mVx9LpW3nT7bYc4dHs6fJa1uEg5iOk\n",
            ),
            ("slack-token", "SLACK=xoxb-2849-1928374650-QwErTyUiOpAsDf\n"),
            ("stripe-key", "STRIPE=sk_live_4eC39HqLyjWDarjtT1zdp7dc8Q\n"),
            (
                "private-key",
                "-----BEGIN OPENSSH PRIVATE KEY-----\nb3Blbn\n",
            ),
            (
                "google-api-key",
                "maps = \"AIzaSyD-9tSrke72PouQMnMX-a7eZSW0jkFMBWY\"\n",
            ),
            (
                "generic-secret",
                "\n\nclient_secret = \"q8Zr2Lm9Vx7Kp4Wn1Tb6Yc3Hd5Ja0Ue\"\n",
            ),
        ];
        for (detector, text) in leaked {
            let hits = found("config/app.env", text, &rules);
            assert!(
                hits.iter().any(|h| h.starts_with(detector)),
                "{detector} not found in {text:?}: {hits:?}"
            );
        }
        assert_eq!(
            found(
                "x",
                "\n\nclient_secret = \"q8Zr2Lm9Vx7Kp4Wn1Tb6Yc3Hd5Ja0Ue\"",
                &rules
            ),
            ["generic-secret:3"],
            "lines are 1-based"
        );
        for clean in [
            "api_key = \"your-api-key-goes-here-please\"\n",
            "password = os.environ[\"PASSWORD\"]\n",
            "AKIAEXAMPLE is not a key\n",
            "let token = request.token();\n",
        ] {
            assert!(found("src/lib.rs", clean, &rules).is_empty(), "{clean}");
        }
    }

    #[test]
    fn rules_allow_paths_and_fingerprints_and_add_patterns() {
        let secret = "ghp_Zr8Kq2mVx9LpW3nT7bYc4dHs6fJa1uEg5iOk";
        let rules = SecretRules::parse(&format!(
            r#"
            mode = "warn"
            allow_paths = ["tests/fixtures/**"]
            allow_fingerprints = ["{}"]
            [[pattern]]
            name = "acme-deploy-key"
            regex = 'acme_dk_[0-9a-f]{{40}}'
            "#,
            fingerprint("AKIAZ7T3KQ2MVXC4L9PW")
        ))
        .unwrap();
        assert_eq!(rules.mode, ScanMode::Warn);
        assert!(found("tests/fixtures/token.txt", secret, &rules).is_empty());
        assert_eq!(found("src/a.rs", secret, &rules), ["github-token:1"]);
        assert!(
            found("src/a.rs", "AKIAZ7T3KQ2MVXC4L9PW", &rules).is_empty(),
            "allow-listed fake"
        );
        assert_eq!(
            found(
                "src/a.rs",
                &format!("k=acme_dk_{}", "ab12".repeat(10)),
                &rules
            ),
            ["acme-deploy-key:1"]
        );
        let finding = &scan("a", secret.as_bytes(), &SecretRules::default())[0];
        assert!(
            !format!("{finding:?}").contains(secret),
            "findings never carry the secret"
        );
        assert!(SecretRules::parse("mode = \"shout\"").is_err());
        assert!(SecretRules::parse("[[pattern]]\nname = \"x\"\nregex = \"(\"").is_err());
    }

    #[test]
    fn binary_looking_and_utf16_content_is_still_scanned() {
        let rules = SecretRules::default();
        let mut nul = b"\0\0header\0".to_vec();
        nul.extend_from_slice(b"\nkey=AKIAZ7T3KQ2MVXC4L9PW\n");
        assert!(!scan("blob.bin", &nul, &rules).is_empty(), "a NUL byte");
        let wide: Vec<u8> = "key=AKIAZ7T3KQ2MVXC4L9PW\r\n"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert!(!scan("app.config", &wide, &rules).is_empty(), "UTF-16");
    }

    #[test]
    fn findings_per_file_are_capped() {
        let text: String = (0..2000u32)
            .map(|i| {
                format!(
                    "k{i} = AKIA{:016X}\n",
                    0x9E37_79B9_7F4A_7C15u64.wrapping_mul(i as u64 + 1)
                )
            })
            .collect();
        let found = scan("keys.txt", text.as_bytes(), &SecretRules::default());
        assert_eq!(found.len(), MAX_FINDINGS);
        assert_eq!(found[1].line, 2, "lines stay right when counted once");
    }
}
