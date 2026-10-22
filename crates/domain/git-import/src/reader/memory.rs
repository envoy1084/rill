//! An in-memory [`GitSource`] for tests and fixtures.

use std::collections::HashMap;

use bytes::Bytes;
use rill_types::{FileKind, Timestamp};

use super::source::{CommitChanges, GitChange, GitCommit, GitSha, GitSource};
use crate::error::{Error, Result};

/// A hand-built git history.
#[derive(Debug, Default, Clone)]
pub struct MemorySource {
    commits: HashMap<GitSha, (GitCommit, CommitChanges)>,
    refs: HashMap<String, GitSha>,
}

impl MemorySource {
    /// An empty history.
    pub fn new() -> Self {
        MemorySource::default()
    }

    /// Add a commit with regular-file sets; `(path, content)` pairs with
    /// empty content are deletions. Returns the sha for chaining.
    pub fn add_commit(&mut self, sha: &str, parents: &[&str], files: &[(&str, &str)]) -> GitSha {
        let changes = files
            .iter()
            .map(|(path, content)| {
                if content.is_empty() {
                    GitChange::Delete {
                        path: path.to_string(),
                    }
                } else {
                    GitChange::Set {
                        path: path.to_string(),
                        content: Bytes::copy_from_slice(content.as_bytes()),
                        kind: FileKind::Regular,
                    }
                }
            })
            .collect();
        self.add_full(
            GitCommit {
                sha: GitSha::new(sha),
                parents: parents.iter().map(GitSha::new).collect(),
                author_name: "tester".into(),
                author_email: "tester@example.com".into(),
                committed_at: Timestamp::from_unix_seconds(1_700_000_000),
                message: format!("commit {sha}"),
            },
            CommitChanges {
                changes,
                warnings: Vec::new(),
            },
        )
    }

    /// Add a fully specified commit.
    pub fn add_full(&mut self, commit: GitCommit, changes: CommitChanges) -> GitSha {
        let sha = commit.sha.clone();
        self.commits.insert(sha.clone(), (commit, changes));
        sha
    }

    /// Point a ref name at a commit.
    pub fn set_ref(&mut self, name: &str, sha: &str) {
        self.refs.insert(name.to_string(), GitSha::new(sha));
    }
}

impl GitSource for MemorySource {
    fn resolve(&mut self, head_ref: &str) -> Result<GitSha> {
        if let Some(sha) = self.refs.get(head_ref) {
            return Ok(sha.clone());
        }
        let sha = GitSha::new(head_ref);
        if self.commits.contains_key(&sha) {
            Ok(sha)
        } else {
            Err(Error::UnknownRef(head_ref.to_string()))
        }
    }

    fn commit(&mut self, sha: &GitSha) -> Result<GitCommit> {
        self.commits
            .get(sha)
            .map(|(c, _)| c.clone())
            .ok_or_else(|| Error::CommitNotFound(sha.clone()))
    }

    fn changes(&mut self, sha: &GitSha) -> Result<CommitChanges> {
        self.commits
            .get(sha)
            .map(|(_, c)| c.clone())
            .ok_or_else(|| Error::CommitNotFound(sha.clone()))
    }
}
