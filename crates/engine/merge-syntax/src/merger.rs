//! The weave-backed [`ContentMerger`].

use std::collections::BTreeSet;
use std::time::Duration;

use rill_core::merge::ContentMerger;
use rill_types::RepoPath;

use crate::config::MergeConfig;
use crate::stats::MergeStats;

/// The weave-core release this build is pinned to.
pub const WEAVE_VERSION: &str = "0.6.1";

/// Resolves text conflicts with weave for every extension it supports,
/// less those the configuration excludes.
pub struct WeaveMerger {
    name: String,
    extensions: Vec<String>,
    max_bytes: usize,
    timeout: Duration,
    stats: MergeStats,
}

impl WeaveMerger {
    pub fn new(config: &MergeConfig) -> Self {
        let excluded: BTreeSet<String> = config
            .exclude
            .iter()
            .map(|e| format!(".{}", e.trim_start_matches('.')))
            .collect();
        let mut extensions: Vec<String> = weave_core::supported_merge_extensions()
            .into_iter()
            .filter(|e| !excluded.contains(e))
            .collect();
        // Longest first, so `.d.ts` wins over `.ts`.
        extensions.sort_by_key(|e| std::cmp::Reverse(e.len()));
        WeaveMerger {
            name: format!("weave-{WEAVE_VERSION}"),
            extensions,
            max_bytes: config.max_bytes,
            timeout: config.timeout(),
            stats: MergeStats::default(),
        }
    }

    /// Every extension it merges.
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// What it has done so far.
    pub fn stats(&self) -> &MergeStats {
        &self.stats
    }

    fn extension_of(&self, path: &RepoPath) -> Option<&str> {
        let name = path.as_str().rsplit('/').next().unwrap_or("");
        self.extensions
            .iter()
            .find(|e| name.ends_with(e.as_str()) && name.len() > e.len())
            .map(String::as_str)
    }
}

/// Warnings weave itself calls advisory: both sides added or changed
/// different members of one container, or inserted disjoint blocks into one
/// slot. Exactly the merges structural merge is for. Every other kind (the
/// result does not parse, a line is duplicated, an entity depends on another
/// the other side changed) declines.
fn advisory(kind: &weave_core::validate::WarningKind) -> bool {
    use weave_core::validate::WarningKind as K;
    matches!(
        kind,
        K::SiblingCoChange { .. } | K::CompositionLicensed { .. }
    )
}

impl ContentMerger for WeaveMerger {
    fn name(&self) -> &str {
        &self.name
    }

    fn handles(&self, path: &RepoPath) -> bool {
        self.extension_of(path).is_some()
    }

    fn merge(&self, path: &RepoPath, base: &[u8], ours: &[u8], theirs: &[u8]) -> Option<Vec<u8>> {
        let extension = self.extension_of(path)?.to_string();
        if [base, ours, theirs]
            .iter()
            .any(|b| b.len() > self.max_bytes)
        {
            self.stats.count(&extension, |s| s.skipped += 1);
            return None;
        }
        let (Ok(base), Ok(ours), Ok(theirs)) = (
            std::str::from_utf8(base),
            std::str::from_utf8(ours),
            std::str::from_utf8(theirs),
        ) else {
            self.stats.count(&extension, |s| s.skipped += 1);
            return None;
        };
        let result = weave_core::entity_merge(base, ours, theirs, path.as_str());
        // Only a clean entity-level merge counts: a conflict, a risk warning
        // or weave's own line fallback all leave the line conflict.
        let accepted = result.is_clean()
            && !result.stats.used_fallback
            && result.warnings.iter().all(|w| advisory(&w.kind));
        if !accepted {
            tracing::debug!(
                %path,
                conflicts = result.conflicts.len(),
                warnings = result.warnings.len(),
                fallback = result.stats.used_fallback,
                "structural merge declined"
            );
            self.stats.count(&extension, |s| s.declined += 1);
            return None;
        }
        tracing::debug!(%path, "structural merge resolved a conflict");
        self.stats.count(&extension, |s| s.resolved += 1);
        Some(result.content.into_bytes())
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use rill_core::merge::{merge_file, FileMergeResult};
    use rill_types::RepoPathBuf;

    use super::*;

    fn path(p: &str) -> RepoPathBuf {
        RepoPathBuf::new(p).unwrap()
    }

    /// Both sides add next to each other: the line merge conflicts, weave
    /// keeps both.
    fn resolves(file: &str, base: &str, ours: &str, theirs: &str, expect: &[&str]) {
        assert!(
            matches!(
                merge_file(base.as_bytes(), ours.as_bytes(), theirs.as_bytes()),
                FileMergeResult::Conflicted { .. }
            ),
            "{file}: the line merge should conflict"
        );
        let merger = WeaveMerger::new(&MergeConfig::default());
        let merged = merger
            .merge(
                &path(file),
                base.as_bytes(),
                ours.as_bytes(),
                theirs.as_bytes(),
            )
            .unwrap_or_else(|| panic!("{file}: weave declined"));
        let merged = String::from_utf8(merged).unwrap();
        for needle in expect {
            assert!(
                merged.contains(needle),
                "{file}: {needle:?} missing in\n{merged}"
            );
        }
        assert_eq!(
            merger
                .stats()
                .snapshot()
                .values()
                .map(|s| s.resolved)
                .sum::<u64>(),
            1
        );
    }

    #[test]
    fn adjacent_additions_merge_in_every_tested_language() {
        resolves(
            "src/lib.rs",
            "use a::A;\n\nfn f() {}\n",
            "use a::A;\nuse b::B;\n\nfn f() {}\n",
            "use a::A;\nuse c::C;\n\nfn f() {}\n",
            &["use b::B;", "use c::C;"],
        );
        resolves(
            "src/kind.rs",
            "enum Kind {\n    A,\n}\n",
            "enum Kind {\n    A,\n    B,\n}\n",
            "enum Kind {\n    A,\n    C,\n}\n",
            &["B,", "C,"],
        );
        resolves(
            "src/two.rs",
            "fn a() {}\n",
            "fn a() {}\n\nfn b() {}\n",
            "fn a() {}\n\nfn c() {}\n",
            &["fn b() {}", "fn c() {}"],
        );
        resolves(
            "Cargo.toml",
            "[dependencies]\nserde = \"1\"\n",
            "[dependencies]\nserde = \"1\"\ntokio = \"1\"\n",
            "[dependencies]\nserde = \"1\"\nbytes = \"1\"\n",
            &["tokio = \"1\"", "bytes = \"1\""],
        );
        resolves(
            "app/imports.py",
            "import os\n\nx = 1\n",
            "import os\nimport re\n\nx = 1\n",
            "import os\nimport sys\n\nx = 1\n",
            &["import re", "import sys"],
        );
        resolves(
            "app/views.py",
            "def a():\n    return 1\n",
            "def a():\n    return 1\n\n\ndef b():\n    return 2\n",
            "def a():\n    return 1\n\n\ndef c():\n    return 3\n",
            &["def b():", "def c():"],
        );
        resolves(
            "web/api.ts",
            "export function a() {\n  return 1;\n}\n",
            "export function a() {\n  return 1;\n}\n\nexport function b() {\n  return 2;\n}\n",
            "export function a() {\n  return 1;\n}\n\nexport function c() {\n  return 3;\n}\n",
            &["function b()", "function c()"],
        );
        resolves(
            "config/app.json",
            "{\n  \"a\": 1\n}\n",
            "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
            "{\n  \"a\": 1,\n  \"c\": 3\n}\n",
            &["\"b\": 2", "\"c\": 3"],
        );
    }

    #[test]
    fn real_conflicts_and_unsupported_inputs_are_declined() {
        let merger = WeaveMerger::new(&MergeConfig::default());
        let base = "fn f() -> u32 {\n    1\n}\n";
        let ours = "fn f() -> u32 {\n    2\n}\n";
        let theirs = "fn f() -> u32 {\n    3\n}\n";
        assert!(merger
            .merge(
                &path("a.rs"),
                base.as_bytes(),
                ours.as_bytes(),
                theirs.as_bytes()
            )
            .is_none());
        assert!(!merger.handles(&path("notes.txt")), "no grammar");
        assert!(
            merger
                .merge(&path("a.rs"), b"\xff\xfe", b"a", b"b")
                .is_none(),
            "not UTF-8"
        );
        let excluded = WeaveMerger::new(&MergeConfig {
            exclude: vec!["rs".into()],
            ..MergeConfig::default()
        });
        assert!(!excluded.handles(&path("src/lib.rs")) && excluded.handles(&path("Cargo.toml")));
        let small = WeaveMerger::new(&MergeConfig {
            max_bytes: 4,
            ..MergeConfig::default()
        });
        assert!(small
            .merge(
                &path("a.rs"),
                base.as_bytes(),
                ours.as_bytes(),
                theirs.as_bytes()
            )
            .is_none());
        assert!(
            merger.extensions().iter().any(|e| e == ".go"),
            "{:?}",
            merger.extensions()
        );
    }
}
