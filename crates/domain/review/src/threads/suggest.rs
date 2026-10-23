//! Suggested edits: a thread on a line range that carries replacement
//! text, which the change's author applies as a new version.

use bytes::Bytes;
use rill_core::diff::split_lines;
use rill_types::{ChangeId, ChangeVersionHash, Principal, RepoPathBuf, ThreadAnchor};

use super::comment::comment;
use crate::error::{Error, Result};
use crate::service::ReviewService;

/// The content a suggestion produces: `path` at `version`, with the
/// anchored lines replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestedEdit {
    pub version: ChangeVersionHash,
    pub path: RepoPathBuf,
    pub content: Bytes,
}

/// `content` with the 1-based inclusive lines `start..=end` replaced by
/// `replacement` (which gets a trailing newline when the replaced lines
/// had one).
pub fn replace_lines(content: &[u8], start: u32, end: u32, replacement: &str) -> Option<Bytes> {
    let lines: Vec<&[u8]> = split_lines(content).collect();
    let (s, e) = (start as usize, end as usize);
    if s == 0 || e < s || e > lines.len() {
        return None;
    }
    let mut out = Vec::with_capacity(content.len() + replacement.len());
    for l in &lines[..s - 1] {
        out.extend_from_slice(l);
    }
    out.extend_from_slice(replacement.as_bytes());
    if lines[e - 1].ends_with(b"\n") && !replacement.is_empty() && !replacement.ends_with('\n') {
        out.push(b'\n');
    }
    for l in &lines[e..] {
        out.extend_from_slice(l);
    }
    Some(Bytes::from(out))
}

impl ReviewService {
    /// Open a thread on a line range of the current version suggesting
    /// `suggestion` in place of those lines.
    pub async fn suggest(
        &self,
        change_id: &ChangeId,
        author: &Principal,
        anchor: ThreadAnchor,
        body: impl Into<String>,
        suggestion: String,
        to: Option<Principal>,
    ) -> Result<String> {
        let current = self.get(change_id).await?.current_version();
        match &anchor {
            ThreadAnchor::Lines { version, .. } if Some(*version) == current => {}
            _ => {
                return Err(Error::BadResult(
                    "a suggestion needs a line range on the current version".into(),
                ))
            }
        }
        let mut first = comment(author, body.into(), None, None);
        first.suggestion = Some(suggestion);
        self.open_thread(change_id, author, anchor, first, to).await
    }

    /// What applying thread `thread_id`'s suggestion produces on the
    /// change's current version: refused when the thread is resolved, has
    /// no suggestion, or its anchor is not on the current version.
    pub async fn suggested_edit(
        &self,
        change_id: &ChangeId,
        thread_id: &str,
    ) -> Result<SuggestedEdit> {
        let state = self.get(change_id).await?;
        let current = state
            .current_version()
            .ok_or(Error::NoVersions(*change_id))?;
        let thread = state
            .threads
            .iter()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| Error::ThreadNotFound {
                change_id: *change_id,
                thread_id: thread_id.to_string(),
            })?;
        let refuse = |why: &str| Err(Error::BadResult(format!("cannot apply: {why}")));
        if thread.resolved {
            return refuse("the thread is resolved");
        }
        let Some(suggestion) = thread.comments.first().and_then(|c| c.suggestion.clone()) else {
            return refuse("the thread suggests nothing");
        };
        let ThreadAnchor::Lines {
            version,
            path,
            start,
            end,
            ..
        } = &thread.anchor
        else {
            return refuse("the thread is not on a line range");
        };
        if *version != current || thread.outdated {
            return refuse("the code it was made on has changed");
        }
        let root = self.root_of(&current).await?;
        let Some(content) = self.file_at(root, path).await? else {
            return refuse("the file is gone");
        };
        let content = replace_lines(&content, *start, *end, &suggestion)
            .ok_or_else(|| Error::BadResult("the line range is outside the file".into()))?;
        Ok(SuggestedEdit {
            version: current,
            path: path.clone(),
            content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_suggestion_replaces_exactly_its_lines() {
        let text = b"a\nb\nc\nd\n";
        assert_eq!(
            &replace_lines(text, 2, 3, "B\nC").unwrap()[..],
            b"a\nB\nC\nd\n"
        );
        assert_eq!(&replace_lines(text, 4, 4, "").unwrap()[..], b"a\nb\nc\n");
        assert_eq!(&replace_lines(b"x", 1, 1, "y").unwrap()[..], b"y");
        assert!(replace_lines(text, 3, 5, "z").is_none());
        assert!(replace_lines(text, 0, 1, "z").is_none());
    }
}
