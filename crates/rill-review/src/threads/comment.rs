//! Opening, replying to and resolving threads.

use rill_types::objects::review::CommentCategory;
use rill_types::{ChangeId, Comment, Principal, ReviewState, Thread, ThreadAnchor, Timestamp};

use super::anchor::context_hash;
use crate::error::{Error, Result};
use crate::service::{ReviewEvent, ReviewService};

/// A new thread id: 16 random bytes, hex encoded.
pub fn new_thread_id() -> String {
    hex::encode(rill_types::ids::random_id())
}

/// What a comment carries besides its body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentMeta {
    /// Category of the finding, if any.
    pub category: Option<CommentCategory>,
    /// Reviewer confidence 0-100.
    pub confidence: Option<u8>,
}

impl ReviewService {
    /// Open a thread at `anchor` with a first comment. For a `Lines` anchor
    /// with an empty `context_hash`, the hash is computed from the file
    /// content at the anchor's version. Returns the new thread id.
    #[allow(clippy::too_many_arguments)]
    pub async fn comment(
        &self,
        change_id: &ChangeId,
        author: &Principal,
        anchor: ThreadAnchor,
        body: impl Into<String>,
        category: Option<CommentCategory>,
        confidence: Option<u8>,
        to: Option<Principal>,
    ) -> Result<String> {
        let anchor = self.fill_context_hash(anchor).await?;
        let thread_id = new_thread_id();
        let thread = Thread {
            id: thread_id.clone(),
            anchor,
            comments: vec![comment(author, body.into(), category, confidence)],
            resolved: false,
            to,
        };
        self.update(change_id, |s| {
            s.threads.push(thread.clone());
            Ok(())
        })
        .await?;
        self.emit(ReviewEvent::Commented {
            change_id: *change_id,
            thread_id: thread_id.clone(),
            author: author.clone(),
            new_thread: true,
        });
        Ok(thread_id)
    }

    /// Add a comment to an existing thread.
    pub async fn reply(
        &self,
        change_id: &ChangeId,
        thread_id: &str,
        author: &Principal,
        body: impl Into<String>,
        meta: CommentMeta,
    ) -> Result<ReviewState> {
        let body = body.into();
        let (state, ()) = self
            .update(change_id, |s| {
                let thread = find_thread(s, change_id, thread_id)?;
                thread.comments.push(comment(
                    author,
                    body.clone(),
                    meta.category,
                    meta.confidence,
                ));
                Ok(())
            })
            .await?;
        self.emit(ReviewEvent::Commented {
            change_id: *change_id,
            thread_id: thread_id.to_string(),
            author: author.clone(),
            new_thread: false,
        });
        Ok(state)
    }

    /// Mark a thread resolved and refresh the status, since blocking
    /// threads gate landing.
    pub async fn resolve(
        &self,
        change_id: &ChangeId,
        thread_id: &str,
        by: &Principal,
    ) -> Result<ReviewState> {
        let current = self.get(change_id).await?;
        let facts = self.change_facts(&current).await?;
        let (state, transition) = self
            .update(change_id, |s| {
                let thread = find_thread(s, change_id, thread_id)?;
                if thread.resolved {
                    return Err(Error::ThreadAlreadyResolved(thread_id.to_string()));
                }
                thread.resolved = true;
                self.refresh_status(s, &facts)
            })
            .await?;
        self.emit(ReviewEvent::ThreadResolved {
            change_id: *change_id,
            thread_id: thread_id.to_string(),
            by: by.clone(),
        });
        self.emit_transition(change_id, transition);
        Ok(state)
    }

    /// Reopen a resolved thread.
    pub async fn unresolve(&self, change_id: &ChangeId, thread_id: &str) -> Result<ReviewState> {
        let current = self.get(change_id).await?;
        let facts = self.change_facts(&current).await?;
        let (state, transition) = self
            .update(change_id, |s| {
                find_thread(s, change_id, thread_id)?.resolved = false;
                self.refresh_status(s, &facts)
            })
            .await?;
        self.emit_transition(change_id, transition);
        Ok(state)
    }

    async fn fill_context_hash(&self, anchor: ThreadAnchor) -> Result<ThreadAnchor> {
        match anchor {
            ThreadAnchor::Lines {
                version,
                path,
                start,
                end,
                context_hash: hash,
            } if hash.is_empty() => {
                let root = self.root_of(&version).await?;
                let content = self.file_at(root, &path).await?.unwrap_or_default();
                Ok(ThreadAnchor::Lines {
                    version,
                    path,
                    start,
                    end,
                    context_hash: context_hash(&content, start, end),
                })
            }
            other => Ok(other),
        }
    }
}

fn comment(
    author: &Principal,
    body: String,
    category: Option<CommentCategory>,
    confidence: Option<u8>,
) -> Comment {
    Comment {
        author: author.clone(),
        body,
        created_at: Timestamp::now(),
        category,
        confidence: confidence.map(|c| c.min(100)),
    }
}

fn find_thread<'a>(
    state: &'a mut ReviewState,
    change_id: &ChangeId,
    thread_id: &str,
) -> Result<&'a mut Thread> {
    state
        .threads
        .iter_mut()
        .find(|t| t.id == thread_id)
        .ok_or_else(|| Error::ThreadNotFound {
            change_id: *change_id,
            thread_id: thread_id.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture;

    #[tokio::test]
    async fn thread_lifecycle() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("a.txt", "1\n").await;
        let (cid, _) = fx.child_change(base, "b.txt", "2\n").await;
        fx.service.submit(&cid, &fx.alice).await.unwrap();
        let tid = fx
            .service
            .comment(
                &cid,
                &fx.alice,
                ThreadAnchor::Change,
                "looks off",
                Some(CommentCategory::Question),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(tid.len(), 32);
        let s = fx
            .service
            .reply(&cid, &tid, &fx.bot, "agreed", CommentMeta::default())
            .await
            .unwrap();
        assert_eq!(s.threads[0].comments.len(), 2);
        assert_eq!(s.unresolved_threads(), 1);
        let s = fx.service.resolve(&cid, &tid, &fx.alice).await.unwrap();
        assert!(s.threads[0].resolved);
        assert!(matches!(
            fx.service.resolve(&cid, &tid, &fx.alice).await,
            Err(Error::ThreadAlreadyResolved(_))
        ));
        assert!(matches!(
            fx.service.resolve(&cid, "nope", &fx.alice).await,
            Err(Error::ThreadNotFound { .. })
        ));
    }
}
