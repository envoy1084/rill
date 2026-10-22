//! Masking secret values in output.

use aho_corasick::{AhoCorasick, MatchKind};

/// Replaces every secret value in a text with `[secret NAME]`.
pub struct Redactor {
    matcher: AhoCorasick,
    names: Vec<String>,
}

impl Redactor {
    /// A redactor for `secrets` (name, value); `None` when there are none.
    pub fn new(secrets: &[(String, Vec<u8>)]) -> Option<Self> {
        let secrets: Vec<_> = secrets.iter().filter(|(_, v)| !v.is_empty()).collect();
        if secrets.is_empty() {
            return None;
        }
        let matcher = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(secrets.iter().map(|(_, value)| value))
            .ok()?;
        Some(Redactor {
            matcher,
            names: secrets.iter().map(|(name, _)| name.clone()).collect(),
        })
    }

    /// `bytes` with every value masked, or `None` when none appears.
    pub fn redact(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        let mut found = false;
        let mut out = Vec::with_capacity(bytes.len());
        self.matcher
            .replace_all_with_bytes(bytes, &mut out, |m, _, out| {
                found = true;
                out.extend_from_slice(format!("[secret {}]", self.names[m.pattern()]).as_bytes());
                true
            });
        found.then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor(pairs: &[(&str, &str)]) -> Redactor {
        let secrets: Vec<_> = pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.as_bytes().to_vec()))
            .collect();
        Redactor::new(&secrets).unwrap()
    }

    #[test]
    fn every_occurrence_of_every_value_is_masked() {
        let r = redactor(&[("TOKEN", "s3cr3t-abc"), ("KEY", "hunter22")]);
        let out = r
            .redact(b"token=s3cr3t-abc key=hunter22 again s3cr3t-abc")
            .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "token=[secret TOKEN] key=[secret KEY] again [secret TOKEN]"
        );
        assert!(r.redact(b"nothing to see").is_none());
    }

    #[test]
    fn a_value_containing_another_is_masked_whole() {
        let r = redactor(&[("SHORT", "abcdef"), ("LONG", "abcdef-123456")]);
        assert_eq!(
            r.redact(b"x abcdef-123456 y").unwrap(),
            b"x [secret LONG] y"
        );
    }
}
