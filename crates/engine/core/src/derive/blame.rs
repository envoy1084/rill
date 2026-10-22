//! Line attribution along the first-parent line.
//!
//! Blame is derived per `(version, path)` on demand: the chain of
//! first-parent versions is walked back until a stored blame, the file's
//! creation, or its absence, and the result is then computed forward one
//! diff at a time, storing every intermediate version so later requests
//! on the same path are incremental.

use std::sync::Arc;

use bytes::Bytes;
use rill_types::objects::change::FileState;
use rill_types::{ChangeVersionHash, ObjectHash, ObjectKind, Principal, RepoPath};
use serde::{Deserialize, Serialize};

use super::manager::DeriveManager;
use super::root_tree::RootTree;
use crate::blobs::read_content;
use crate::diff::lines::edit_script;
use crate::diff::tokens::LineTokens;
use crate::diff::{file_state, is_binary};
use crate::error::{Error, Result};

/// Attribution of one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlameLine {
    /// Version that introduced the line.
    pub version: ChangeVersionHash,
    pub author: Principal,
    /// One-based line number the line had in `version`.
    pub line_no: u32,
}

/// Blame storage parameters.
pub struct Blame;

impl Blame {
    /// Storage kind, including the computation version.
    pub const KIND: &'static str = "blame@1";

    /// Derived-data key for `(version, path)`.
    pub fn key(version: &ChangeVersionHash, path: &RepoPath) -> ObjectHash {
        let mut h = ObjectHash::hasher(ObjectKind::Derived);
        h.update(b"blame:")
            .update(version.as_bytes())
            .update(path.as_str().as_bytes());
        h.finalize()
    }
}

struct Step {
    version: ChangeVersionHash,
    author: Principal,
    state: FileState,
}

impl DeriveManager {
    /// Blame for `path` as of `version`: one entry per line of the file.
    pub async fn blame(
        self: &Arc<Self>,
        version: &ChangeVersionHash,
        path: &RepoPath,
    ) -> Result<Vec<BlameLine>> {
        // Walk back collecting the versions whose blame is not yet stored.
        let mut chain: Vec<Step> = Vec::new();
        let mut cursor = Some(*version);
        let mut base: (Vec<BlameLine>, Bytes) = (Vec::new(), Bytes::new());
        while let Some(at) = cursor {
            if let Some(stored) = self
                .get_keyed::<Vec<BlameLine>>(Blame::KIND, &Blame::key(&at, path))
                .await?
            {
                let root = self.get::<RootTree>(&at).await?;
                let state = file_state(self.trees(), Some(root), path).await?;
                let bytes = match &state {
                    Some((content, _)) => {
                        read_content(self.repo().objects().as_ref(), content).await?
                    }
                    None => Bytes::new(),
                };
                base = (stored, bytes);
                break;
            }
            let cv = self.change(&at).await?;
            let root = self.get::<RootTree>(&at).await?;
            let state = file_state(self.trees(), Some(root), path).await?;
            if state.is_none() {
                if chain.is_empty() {
                    return Err(Error::PathNotFound(path.to_owned()));
                }
                break;
            }
            chain.push(Step {
                version: at,
                author: cv.author.clone(),
                state,
            });
            cursor = cv.first_parent();
        }

        let (mut blame, mut prev_bytes) = base;
        let mut prev_hash: Option<ObjectHash> = None;
        for step in chain.into_iter().rev() {
            // Present: absent states end the walk above.
            let Some((content, _)) = &step.state else {
                unreachable!()
            };
            let hash = content.content_hash();
            if prev_hash != Some(hash) {
                let bytes = read_content(self.repo().objects().as_ref(), content).await?;
                if is_binary(&bytes) {
                    return Err(Error::BinaryContent(path.to_owned()));
                }
                blame = advance(&blame, &prev_bytes, &bytes, &step);
                prev_bytes = bytes;
                prev_hash = Some(hash);
            }
            self.put_keyed(Blame::KIND, &Blame::key(&step.version, path), &blame)
                .await?;
        }
        Ok(blame)
    }
}

/// Carry attributions across unchanged lines and attribute added lines to
/// `step`.
fn advance(prev: &[BlameLine], old: &[u8], new: &[u8], step: &Step) -> Vec<BlameLine> {
    let old_tokens = LineTokens::new(old);
    let new_tokens = LineTokens::new(new);
    let edits = edit_script(old_tokens.lines(), new_tokens.lines());
    let mut out = Vec::with_capacity(new_tokens.len());
    let mut old_pos = 0;
    let mut new_pos = 0;
    for (old_range, new_range) in edits {
        while old_pos < old_range.start {
            out.push(carry(prev, old_pos, new_pos, step));
            old_pos += 1;
            new_pos += 1;
        }
        for _ in new_range.clone() {
            new_pos += 1;
            out.push(BlameLine {
                version: step.version,
                author: step.author.clone(),
                line_no: new_pos as u32,
            });
        }
        old_pos = old_range.end;
    }
    while old_pos < old_tokens.len() {
        out.push(carry(prev, old_pos, new_pos, step));
        old_pos += 1;
        new_pos += 1;
    }
    out
}

/// The previous attribution of an unchanged line; a stored blame that is
/// shorter than its file (corrupt or from an older computation) falls back
/// to attributing the line to `step`.
fn carry(prev: &[BlameLine], old_pos: usize, new_pos: usize, step: &Step) -> BlameLine {
    prev.get(old_pos).cloned().unwrap_or_else(|| BlameLine {
        version: step.version,
        author: step.author.clone(),
        line_no: new_pos as u32 + 1,
    })
}
