//! The built-in detectors: gitleaks' maintained rule set, vendored in
//! `rules/gitleaks.toml` and applied as gitleaks applies it, plus the
//! credential formats Rill itself issues.

use std::sync::LazyLock;

use aho_corasick::AhoCorasick;
use regex::{Regex, RegexSet};
use serde::Deserialize;

const GITLEAKS: &str = include_str!("../../rules/gitleaks.toml");

/// One detector.
#[derive(Debug)]
pub struct Detector {
    pub name: String,
    pub regex: Regex,
    /// The capture group holding the secret; else the first group, else
    /// the whole match.
    pub secret_group: Option<usize>,
    pub min_entropy: f64,
    /// The rule only runs on content containing one of these (any case).
    pub keywords: Vec<String>,
    /// The rule only applies to matching paths.
    pub path: Option<Regex>,
    allowlists: Vec<Allowlist>,
}

#[derive(Debug, Default)]
struct Allowlist {
    regexes: Vec<Regex>,
    stopwords: Vec<String>,
    paths: Vec<Regex>,
    /// Every listed kind must match, not any one.
    all: bool,
    /// Regexes apply to the whole match rather than the secret.
    on_match: bool,
}

impl Allowlist {
    fn allows(&self, path: &str, secret: &str, matched: &str) -> bool {
        let target = if self.on_match { matched } else { secret };
        let lower = secret.to_ascii_lowercase();
        let checks = [
            (!self.regexes.is_empty()).then(|| self.regexes.iter().any(|r| r.is_match(target))),
            (!self.stopwords.is_empty()).then(|| self.stopwords.iter().any(|w| lower.contains(w))),
            (!self.paths.is_empty()).then(|| self.paths.iter().any(|r| r.is_match(path))),
        ];
        let mut present = checks.iter().flatten().peekable();
        if present.peek().is_none() {
            return false;
        }
        if self.all {
            present.all(|c| *c)
        } else {
            present.any(|c| *c)
        }
    }
}

impl Detector {
    /// Whether an allowlist (the rule's or the catalog's) lets `secret`
    /// found at `path` through.
    pub(super) fn allowed(&self, path: &str, secret: &str, matched: &str) -> bool {
        self.allowlists
            .iter()
            .chain(std::iter::once(&CATALOG.global))
            .any(|a| a.allows(path, secret, matched))
    }
}

/// Every built-in detector, with a keyword index choosing which run.
pub struct Catalog {
    pub detectors: Vec<Detector>,
    keywords: AhoCorasick,
    /// For each keyword, the detectors it enables.
    keyword_detectors: Vec<Vec<usize>>,
    /// Detectors without keywords, which always run.
    always: Vec<usize>,
    /// Paths gitleaks never scans (lockfiles, vendored code, binaries).
    skip_paths: RegexSet,
    global: Allowlist,
    /// Rules the regex engine refused.
    pub skipped: usize,
}

impl Catalog {
    /// Whether `path` is never scanned.
    pub fn skips(&self, path: &str) -> bool {
        self.skip_paths.is_match(path)
    }

    /// The detectors to run over `text`.
    pub fn candidates(&self, text: &str) -> Vec<usize> {
        let mut chosen = vec![false; self.detectors.len()];
        for &i in &self.always {
            chosen[i] = true;
        }
        for m in self.keywords.find_overlapping_iter(text) {
            for &i in &self.keyword_detectors[m.pattern().as_usize()] {
                chosen[i] = true;
            }
        }
        (0..chosen.len()).filter(|&i| chosen[i]).collect()
    }
}

/// The built-in detectors, compiled once.
pub static CATALOG: LazyLock<Catalog> = LazyLock::new(|| load(GITLEAKS));

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    allowlist: Option<RawAllowlist>,
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRule {
    id: String,
    regex: Option<String>,
    secret_group: Option<usize>,
    entropy: Option<f64>,
    #[serde(default)]
    keywords: Vec<String>,
    path: Option<String>,
    #[serde(default)]
    allowlists: Vec<RawAllowlist>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawAllowlist {
    #[serde(default)]
    regexes: Vec<String>,
    #[serde(default)]
    stopwords: Vec<String>,
    #[serde(default)]
    paths: Vec<String>,
    condition: Option<String>,
    regex_target: Option<String>,
}

fn compile(pattern: &str) -> Option<Regex> {
    regex::RegexBuilder::new(pattern)
        .size_limit(50 << 20)
        .build()
        .ok()
}

fn allowlist(raw: RawAllowlist) -> Allowlist {
    Allowlist {
        regexes: raw.regexes.iter().filter_map(|r| compile(r)).collect(),
        stopwords: raw
            .stopwords
            .iter()
            .map(|w| w.to_ascii_lowercase())
            .collect(),
        paths: raw.paths.iter().filter_map(|r| compile(r)).collect(),
        all: raw.condition.as_deref() == Some("AND"),
        on_match: raw.regex_target.as_deref() == Some("match"),
    }
}

/// Rill's own credentials: refresh tokens and SCIM credentials.
fn rill_detectors() -> Vec<Detector> {
    [
        (
            "rill-refresh-token",
            r"\b(ss_[0-9a-f]{32}\.[A-Za-z0-9_-]{20,})\b",
            vec!["ss_"],
        ),
        (
            "rill-scim-credential",
            r"\b(scim_[0-9a-f]{32}\.[A-Za-z0-9_-]{20,})\b",
            vec!["scim_"],
        ),
    ]
    .into_iter()
    .map(|(name, pattern, keywords)| Detector {
        name: name.to_string(),
        regex: Regex::new(pattern).expect("built-in pattern"),
        secret_group: Some(1),
        min_entropy: 3.5,
        keywords: keywords.into_iter().map(str::to_string).collect(),
        path: None,
        allowlists: Vec::new(),
    })
    .collect()
}

fn load(text: &str) -> Catalog {
    let raw: Raw = toml::from_str(text).expect("the vendored gitleaks rules parse");
    let mut detectors = rill_detectors();
    let mut skipped = 0;
    for rule in raw.rules {
        // Path-only rules flag a file's existence, not its content.
        let Some(pattern) = rule.regex else { continue };
        let (Some(regex), path) = (compile(&pattern), rule.path.as_deref().map(compile)) else {
            skipped += 1;
            continue;
        };
        if path.as_ref().is_some_and(Option::is_none) {
            skipped += 1;
            continue;
        }
        detectors.push(Detector {
            name: rule.id,
            regex,
            secret_group: rule.secret_group,
            min_entropy: rule.entropy.unwrap_or(0.0),
            keywords: rule
                .keywords
                .iter()
                .map(|k| k.to_ascii_lowercase())
                .collect(),
            path: path.flatten(),
            allowlists: rule.allowlists.into_iter().map(allowlist).collect(),
        });
    }
    let mut keywords: Vec<String> = Vec::new();
    let mut keyword_detectors: Vec<Vec<usize>> = Vec::new();
    let mut always = Vec::new();
    for (i, d) in detectors.iter().enumerate() {
        if d.keywords.is_empty() {
            always.push(i);
        }
        for k in &d.keywords {
            match keywords.iter().position(|x| x == k) {
                Some(at) => keyword_detectors[at].push(i),
                None => {
                    keywords.push(k.clone());
                    keyword_detectors.push(vec![i]);
                }
            }
        }
    }
    let global = raw.allowlist.unwrap_or_default();
    let skip_paths = RegexSet::new(global.paths.iter().filter(|p| compile(p).is_some()))
        .expect("each path compiled alone");
    Catalog {
        keywords: AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(&keywords)
            .expect("keywords are plain strings"),
        keyword_detectors,
        always,
        skip_paths,
        global: allowlist(RawAllowlist {
            paths: Vec::new(),
            ..global
        }),
        detectors,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vendored_rules_load() {
        assert!(CATALOG.detectors.len() > 150, "{}", CATALOG.detectors.len());
        assert!(
            CATALOG.skipped * 20 < CATALOG.detectors.len(),
            "{} refused",
            CATALOG.skipped
        );
        assert!(CATALOG.skips("web/package-lock.json"));
    }
}
