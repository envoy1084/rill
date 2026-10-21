//! The [`Importer`]: drives a [`GitSource`] from a blocking thread and
//! writes change versions as commits arrive.

use std::collections::{HashMap, HashSet};

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, FileChange, Principal, Provenance, RepoPathBuf,
};
use tokio::sync::mpsc;

use super::land::land_first_parent_history;
use super::map::ImportMap;
use super::options::{ImportOptions, ImportReport};
use super::target::ImportTarget;
use crate::error::{Error, Result};
use crate::reader::{history, CommitChanges, GitChange, GitCommit, GitSha, GitSource};

/// Key under which the git sha is stored in `ChangeVersion::extra`.
pub const GIT_SHA_EXTRA: &str = "git_sha";

/// Provenance keys keeping the git author and commit time.
pub const GIT_AUTHOR_PROVENANCE: &str = "git_author";
pub const GIT_DATE_PROVENANCE: &str = "git_date";

/// Tool name recorded in provenance.
const TOOL: &str = "rill-git-import";

/// Imports git history into one repository, incrementally.
pub struct Importer<T: ImportTarget> {
    repo: T,
    map: ImportMap,
}

/// What the reader thread hands to the writer.
enum Item {
    Head(GitSha),
    Known(GitCommit),
    New(GitCommit, CommitChanges),
}

impl<T: ImportTarget> Importer<T> {
    /// An importer whose map is loaded from `repo`.
    pub async fn open(repo: T) -> Result<Self> {
        let map = repo.load_map().await?;
        Ok(Importer { repo, map })
    }

    /// An importer with an explicit map.
    pub fn new(repo: T, map: ImportMap) -> Self {
        Importer { repo, map }
    }

    /// The current mapping.
    pub fn map(&self) -> &ImportMap {
        &self.map
    }

    /// Import everything reachable from `head_ref` that is not yet in the
    /// map, then (by default) land the mainline on trunk. The map is saved
    /// even when the import fails part-way so a retry resumes.
    pub async fn import<S: GitSource>(
        &mut self,
        source: S,
        head_ref: &str,
        options: ImportOptions,
    ) -> Result<ImportReport> {
        let result = self.run(source, head_ref, &options).await;
        self.repo.save_map(&self.map).await?;
        result
    }

    async fn run<S: GitSource>(
        &mut self,
        source: S,
        head_ref: &str,
        options: &ImportOptions,
    ) -> Result<ImportReport> {
        let known: HashSet<GitSha> = self.map.shas().cloned().collect();
        let (tx, mut rx) = mpsc::channel::<Result<Item>>(options.read_ahead.max(1));
        let head_ref = head_ref.to_string();
        let reader = tokio::task::spawn_blocking(move || read_loop(source, &head_ref, &known, &tx));

        let mut report = ImportReport::default();
        let mut commits: Vec<GitCommit> = Vec::new();
        let mut head_sha: Option<GitSha> = None;
        let mut since_save = 0;
        while let Some(item) = rx.recv().await {
            match item? {
                Item::Head(sha) => head_sha = Some(sha),
                Item::Known(commit) => {
                    report.skipped += 1;
                    commits.push(commit);
                }
                Item::New(commit, changes) => {
                    report.warnings.extend(changes.warnings.iter().cloned());
                    let hash = self.import_commit(&commit, changes).await?;
                    self.map.insert(commit.sha.clone(), hash);
                    report.imported += 1;
                    since_save += 1;
                    if since_save >= options.save_every.max(1) {
                        self.repo.save_map(&self.map).await?;
                        since_save = 0;
                    }
                    commits.push(commit);
                }
            }
        }
        reader.await?;
        let head_sha = head_sha.ok_or(Error::ReaderStopped)?;
        report.head = self.map.get(&head_sha);
        tracing::info!(
            imported = report.imported,
            skipped = report.skipped,
            head = %head_sha,
            "git import complete"
        );

        if options.land_first_parent_history {
            let chain = self.first_parent_chain(&commits, &head_sha)?;
            let (landed, head) = land_first_parent_history(&self.repo, &chain).await?;
            report.landed = landed;
            if let Some((seq, hash)) = head {
                report.seq = Some(seq);
                report.trunk_head = Some(hash);
            }
        }
        Ok(report)
    }

    /// `(version, first parent version)` of every mainline ancestor of
    /// `head`, oldest first.
    fn first_parent_chain(
        &self,
        commits: &[GitCommit],
        head: &GitSha,
    ) -> Result<Vec<(ChangeVersionHash, Option<ChangeVersionHash>)>> {
        let by_sha: HashMap<&GitSha, &GitCommit> = commits.iter().map(|c| (&c.sha, c)).collect();
        let mut chain = Vec::new();
        let mut current = Some(head);
        while let Some(sha) = current {
            let commit = by_sha
                .get(sha)
                .ok_or_else(|| Error::CommitNotFound(sha.clone()))?;
            let version = self.mapped(sha, sha)?;
            let parent = match commit.parents.first() {
                Some(p) => Some(self.mapped(sha, p)?),
                None => None,
            };
            chain.push((version, parent));
            current = commit.parents.first();
        }
        chain.reverse();
        Ok(chain)
    }

    fn mapped(&self, commit: &GitSha, sha: &GitSha) -> Result<ChangeVersionHash> {
        self.map.get(sha).ok_or_else(|| Error::MissingParent {
            commit: commit.clone(),
            parent: sha.clone(),
        })
    }

    async fn import_commit(
        &self,
        commit: &GitCommit,
        changes: CommitChanges,
    ) -> Result<ChangeVersionHash> {
        let mut author = Principal::human(if commit.author_name.is_empty() {
            commit.author_email.clone()
        } else {
            commit.author_name.clone()
        });
        author.identity = Some(commit.author_email.clone());
        let mut provenance = Provenance {
            tool: Some(TOOL.to_string()),
            ..Default::default()
        };
        // A server stamps the importer as the author; the git author and
        // date stay with the version.
        provenance.extra.insert(
            GIT_AUTHOR_PROVENANCE.to_string(),
            format!("{} <{}>", commit.author_name, commit.author_email),
        );
        provenance.extra.insert(
            GIT_DATE_PROVENANCE.to_string(),
            commit.committed_at.millis().to_string(),
        );
        let mut parents = Vec::with_capacity(commit.parents.len());
        for parent in &commit.parents {
            parents.push(self.mapped(&commit.sha, parent)?);
        }
        let mut file_changes = BTreeMap::new();
        for change in changes.changes {
            let path = change.path().to_string();
            let repo_path =
                RepoPathBuf::new(&path).map_err(|e| Error::InvalidPath(path.clone(), e))?;
            let file_change = match change {
                GitChange::Set { content, kind, .. } => {
                    FileChange::set(self.repo.store_content(content).await?, kind)
                }
                GitChange::Delete { .. } => FileChange::Delete,
            };
            file_changes.insert(repo_path, file_change);
        }
        let mut extra = BTreeMap::new();
        extra.insert(
            GIT_SHA_EXTRA.to_string(),
            Bytes::from(commit.sha.to_bytes()),
        );
        let cv = ChangeVersion {
            change_id: ChangeId::derived("git", commit.sha.as_str()),
            parents,
            predecessor: None,
            subtree_changes: BTreeMap::new(),
            file_changes,
            author,
            provenance,
            intent: None,
            message: commit.message.clone(),
            created_at: commit.committed_at,
            extra,
        };
        let hash = self.repo.store_version(&cv).await?;
        tracing::debug!(sha = %commit.sha, version = %hash, files = cv.file_changes.len(), "imported commit");
        Ok(hash)
    }
}

/// Body of the reader thread: resolve, order, and stream commits. Stops
/// quietly when the writer hangs up.
fn read_loop<S: GitSource>(
    mut source: S,
    head_ref: &str,
    known: &HashSet<GitSha>,
    tx: &mpsc::Sender<Result<Item>>,
) {
    let mut produce = || -> Result<()> {
        let head = source.resolve(head_ref)?;
        if tx.blocking_send(Ok(Item::Head(head.clone()))).is_err() {
            return Ok(());
        }
        for commit in history(&mut source, &head)? {
            let item = if known.contains(&commit.sha) {
                Item::Known(commit)
            } else {
                let changes = source.changes(&commit.sha)?;
                Item::New(commit, changes)
            };
            if tx.blocking_send(Ok(item)).is_err() {
                return Ok(());
            }
        }
        Ok(())
    };
    if let Err(e) = produce() {
        // The writer may already be gone; nothing more to report then.
        let _ = tx.blocking_send(Err(e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::MemorySource;
    use rill_core::{DeriveManager, RootTree, TreeOps};
    use rill_store::{RepoStore, Stores};
    use rill_types::{EntryKind, RepoPathBuf, TrunkSeq};

    async fn repo() -> RepoStore {
        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        stores.repo(config.id)
    }

    fn source() -> MemorySource {
        let mut src = MemorySource::new();
        src.add_commit("0a", &[], &[("README", "one\n"), ("src/x", "x1")]);
        src.add_commit("0b", &["0a"], &[("src/x", "x2")]);
        src.add_commit("0c", &["0a"], &[("side", "s")]);
        src.add_commit("0d", &["0b", "0c"], &[("side", "s"), ("README", "")]);
        src.set_ref("main", "0d");
        src
    }

    #[tokio::test]
    async fn imports_merges_and_lands_mainline() {
        let repo = repo().await;
        let mut importer = Importer::open(repo.clone()).await.unwrap();
        let report = importer
            .import(source(), "main", ImportOptions::default())
            .await
            .unwrap();
        assert_eq!(report.imported, 4);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.landed, 3);
        assert_eq!(report.seq, Some(TrunkSeq(3)));
        assert_eq!(report.trunk_head, report.head);

        let d = report.head.unwrap();
        let cv = repo.get_change_version(&d).await.unwrap().unwrap();
        assert_eq!(cv.parents.len(), 2);
        assert_eq!(cv.change_id, ChangeId::derived("git", "0d"));
        assert_eq!(cv.extra[GIT_SHA_EXTRA], Bytes::from(vec![0x0d]));
        assert_eq!(cv.author.identity.as_deref(), Some("tester@example.com"));
        assert_eq!(cv.provenance.tool.as_deref(), Some(TOOL));

        let trees = TreeOps::new(repo.objects().clone(), 16);
        let derive = DeriveManager::new(repo.clone(), trees.clone(), 16);
        let root = derive.get::<RootTree>(&d).await.unwrap();
        assert!(trees
            .lookup(root, &RepoPathBuf::new("README").unwrap())
            .await
            .unwrap()
            .is_none());
        let Some(EntryKind::File { content, .. }) = trees
            .lookup(root, &RepoPathBuf::new("src/x").unwrap())
            .await
            .unwrap()
        else {
            panic!("src/x missing");
        };
        assert_eq!(
            rill_core::read_content(repo.objects().as_ref(), &content)
                .await
                .unwrap(),
            Bytes::from_static(b"x2")
        );

        // Mainline is a, b, d; c is imported but not on trunk.
        let c = importer.map().get(&GitSha::new("0c")).unwrap();
        assert!(repo.trunk_seq_of(&c).await.unwrap().is_none());
        let persisted = ImportMap::load(&repo).await.unwrap();
        assert_eq!(persisted.len(), 4);

        // Re-import is a no-op.
        let mut again = Importer::open(repo.clone()).await.unwrap();
        let report = again
            .import(source(), "main", ImportOptions::default())
            .await
            .unwrap();
        assert_eq!(report.imported, 0);
        assert_eq!(report.skipped, 4);
        assert_eq!(report.landed, 0);
        assert_eq!(report.seq, Some(TrunkSeq(3)));
    }

    #[tokio::test]
    async fn incremental_import_extends_trunk() {
        let repo = repo().await;
        let mut importer = Importer::open(repo.clone()).await.unwrap();
        importer
            .import(source(), "0b", ImportOptions::default())
            .await
            .unwrap();
        assert_eq!(repo.trunk_head().await.unwrap().unwrap().0, TrunkSeq(2));
        let report = importer
            .import(source(), "main", ImportOptions::default())
            .await
            .unwrap();
        assert_eq!(report.imported, 2);
        assert_eq!(report.skipped, 2);
        assert_eq!(report.landed, 1);
        assert_eq!(report.seq, Some(TrunkSeq(3)));
    }

    #[tokio::test]
    async fn refuses_to_land_on_diverged_trunk() {
        let repo = repo().await;
        let mut importer = Importer::open(repo.clone()).await.unwrap();
        importer
            .import(source(), "0b", ImportOptions::default())
            .await
            .unwrap();
        let err = importer
            .import(source(), "0c", ImportOptions::default())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::TrunkDiverged { .. }));
        // The commit itself was imported and remembered.
        assert!(ImportMap::load(&repo)
            .await
            .unwrap()
            .contains(&GitSha::new("0c")));
    }

    #[tokio::test]
    async fn unknown_ref_and_bad_path_are_errors() {
        let repo = repo().await;
        let mut importer = Importer::open(repo.clone()).await.unwrap();
        let err = importer
            .import(source(), "nope", ImportOptions::default())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::UnknownRef(_)));

        let mut bad = MemorySource::new();
        bad.add_commit("0a", &[], &[("ok/../bad", "x")]);
        let err = importer
            .import(bad, "0a", ImportOptions::default())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidPath(p, _) if p == "ok/../bad"));
    }
}
