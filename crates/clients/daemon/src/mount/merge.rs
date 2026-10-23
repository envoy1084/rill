//! Three-way merges of a locally edited file with its upstream change,
//! line-based first and structural when lines conflict.

use std::sync::{Arc, OnceLock};

use bytes::Bytes;
use rill_core::merge::{materialize, merge_file, ContentMerger, FileMergeResult, Structural};
use rill_types::{Merge, RepoPath};

/// A merge's output: the text to write and whether it holds markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// Merged text, or text with conflict markers.
    pub text: Bytes,
    /// True when markers were written.
    pub conflict: bool,
}

/// The structural merger, unless `RILL_STRUCTURAL_MERGE` is `false`.
fn structural_merger() -> Option<&'static Arc<dyn ContentMerger>> {
    static MERGER: OnceLock<Option<Arc<dyn ContentMerger>>> = OnceLock::new();
    MERGER
        .get_or_init(|| {
            let off = std::env::var("RILL_STRUCTURAL_MERGE")
                .is_ok_and(|v| matches!(v.as_str(), "0" | "false" | "no" | "off"));
            (!off).then(|| {
                Arc::new(rill_merge_syntax::WeaveMerger::new(&Default::default()))
                    as Arc<dyn ContentMerger>
            })
        })
        .as_ref()
}

/// Merge `local` and `upstream`, both edits of `base`.
pub async fn merge_texts(path: &RepoPath, base: Bytes, local: Bytes, upstream: Bytes) -> Merged {
    let mut merged = merge_file(&base, &local, &upstream);
    if let (FileMergeResult::Conflicted { .. }, Some(merger)) = (&merged, structural_merger()) {
        let structural = Structural { merger, path };
        if let Some(bytes) = structural
            .resolve(base.clone(), local.clone(), upstream.clone())
            .await
        {
            merged = FileMergeResult::Resolved(bytes);
        }
    }
    match merged {
        FileMergeResult::Resolved(bytes) => Merged {
            text: Bytes::from(bytes),
            conflict: false,
        },
        FileMergeResult::Conflicted { .. } => {
            conflict_text(Some(base), Some(local), Some(upstream))
        }
    }
}

/// Markers for a conflict where a side may be absent (deleted).
pub fn conflict_text(base: Option<Bytes>, local: Option<Bytes>, upstream: Option<Bytes>) -> Merged {
    let terms = Merge::three_way(base, local, upstream);
    Merged {
        text: Bytes::from(materialize(&terms)),
        conflict: true,
    }
}
