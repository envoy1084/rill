//! Content mergers beyond the line merge: a syntax-aware merger gets a
//! file only after the line merge left it conflicted, so every result the
//! line merge produces cleanly is unchanged.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rill_types::{ChangeVersion, RepoPath, RepoPathBuf};

/// The extra field naming the files a version's merger resolved, and the
/// merger: `weave-0.6.1: src/a.rs, Cargo.toml`.
pub const STRUCTURAL_MERGE_EXTRA: &str = "structural_merge";

/// Resolves a three-way text conflict, or declines.
pub trait ContentMerger: Send + Sync + 'static {
    /// Name and version, recorded on what it resolves.
    fn name(&self) -> &str;

    /// Whether it may try `path`.
    fn handles(&self, path: &RepoPath) -> bool;

    /// The merged bytes, or `None` to leave the conflict.
    fn merge(&self, path: &RepoPath, base: &[u8], ours: &[u8], theirs: &[u8]) -> Option<Vec<u8>>;

    /// Longest one merge may take before it is abandoned.
    fn timeout(&self) -> Duration;
}

/// The merger to consult for `path`, when there is one.
#[derive(Clone)]
pub struct Structural<'a> {
    pub merger: &'a Arc<dyn ContentMerger>,
    pub path: &'a RepoPath,
}

impl Structural<'_> {
    /// Run the merger off the async threads, bounded by its timeout. A
    /// panic, a timeout or a decline all leave the conflict.
    pub async fn resolve(&self, base: Bytes, ours: Bytes, theirs: Bytes) -> Option<Vec<u8>> {
        if !self.merger.handles(self.path) {
            return None;
        }
        let merger = self.merger.clone();
        let path = self.path.to_owned();
        let task = tokio::task::spawn_blocking(move || merger.merge(&path, &base, &ours, &theirs));
        match tokio::time::timeout(self.merger.timeout(), task).await {
            Ok(Ok(merged)) => merged,
            Ok(Err(e)) => {
                tracing::warn!(path = %self.path, error = %e, "structural merge failed");
                None
            }
            Err(_) => {
                tracing::warn!(path = %self.path, "structural merge timed out");
                None
            }
        }
    }
}

/// Record on `cv` which of its files `merger` resolved, replacing what an
/// earlier version recorded.
pub fn record_structural(cv: &mut ChangeVersion, merger: &str, paths: &BTreeSet<RepoPathBuf>) {
    cv.extra.remove(STRUCTURAL_MERGE_EXTRA);
    if paths.is_empty() {
        return;
    }
    let mut text = format!("{merger}:");
    for (i, path) in paths.iter().enumerate() {
        let next = format!("{} {path}", if i == 0 { "" } else { "," });
        if text.len() + next.len() > rill_types::limits::MAX_EXTRA_VALUE_BYTES - 16 {
            text.push_str(&format!(" and {} more", paths.len() - i));
            break;
        }
        text.push_str(&next);
    }
    cv.extra.insert(
        STRUCTURAL_MERGE_EXTRA.into(),
        Bytes::from(text.into_bytes()),
    );
}
