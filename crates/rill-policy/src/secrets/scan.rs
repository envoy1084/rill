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

/// Whether `bytes` look binary (a NUL in the first 8 KiB).
fn binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}

/// The credentials in `content` at `path` under `rules`. Binary content
/// and allowed paths are skipped.
pub fn scan(path: &str, content: &[u8], rules: &SecretRules) -> Vec<Finding> {
    if binary(content) || rules.path_allowed(path) {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(content);
    let mut findings = Vec::new();
    let mut add = |name: &str, start: usize, secret: &str| {
        let fp = fingerprint(secret);
        if rules.allow_fingerprints.contains(&fp)
            || findings.iter().any(|f: &Finding| f.fingerprint == fp)
        {
            return;
        }
        findings.push(Finding {
            path: path.to_string(),
            line: text[..start].bytes().filter(|b| *b == b'\n').count() + 1,
            detector: name.to_string(),
            fingerprint: fp,
        });
    };
    for d in BUILT_IN.iter() {
        for caps in d.regex.captures_iter(&text) {
            let m = caps
                .get(1)
                .or_else(|| caps.get(0))
                .expect("a match has group 0");
            if entropy(m.as_str()) >= d.min_entropy {
                add(d.name, m.start(), m.as_str());
            }
        }
    }
    for (name, regex) in &rules.patterns {
        for caps in regex.captures_iter(&text) {
            let m = caps
                .get(1)
                .or_else(|| caps.get(0))
                .expect("a match has group 0");
            add(name, m.start(), m.as_str());
        }
    }
    findings
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
        assert!(scan("bin", b"\0\0AKIAZ7T3KQ2MVXC4L9PW", &SecretRules::default()).is_empty());
    }
}
