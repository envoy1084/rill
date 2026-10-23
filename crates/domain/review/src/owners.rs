//! Trunk's `OWNERS.toml` files, compiled once per trunk version.
//!
//! The index of a version is its first parent's when the version touched
//! no `OWNERS.toml`, the parent's patched with the files it changed when
//! it did, and a walk of the whole tree only when no ancestor nearby has
//! one. Each is kept as derived data, so trunk usually pays one lookup.

use std::sync::Arc;

use rill_core::RootTree;
use rill_policy::owners::{OwnersIndex, OwnersTexts, SetCoverage, OWNERS_FILE};
use rill_types::{
    ChangeVersion, ChangeVersionHash, EntryKind, FileChange, RepoPathBuf, ReviewState,
};

use crate::error::Result;
use crate::service::ReviewService;

/// Derived-data kind under which a version's owners files are kept.
const OWNERS_KIND: &str = "owners@1";
/// Ancestors searched for a cached index before walking the tree.
const MAX_CHAIN: usize = 256;

impl ReviewService {
    /// The owners index at trunk's head; empty on an empty repository.
    pub async fn trunk_owners(&self) -> Result<Arc<OwnersIndex>> {
        let Some((_, head)) = self.repo.trunk_head().await? else {
            return Ok(Arc::default());
        };
        if let Some((at, index)) = self.owners.lock().as_ref() {
            if *at == head {
                return Ok(index.clone());
            }
        }
        let index = Arc::new(OwnersIndex::compile(self.owners_texts(&head).await?)?);
        *self.owners.lock() = Some((head, index.clone()));
        Ok(index)
    }

    /// Each owner set the review's current version touches, with how many
    /// approvals it has: what the submit policy sees.
    pub async fn owner_coverage(&self, state: &ReviewState) -> Result<Vec<SetCoverage>> {
        let facts = self.change_facts(state).await?;
        Ok(rill_policy::submit::owner_coverage(state, &facts))
    }

    /// The owners files of `version`'s tree.
    pub async fn owners_texts(&self, version: &ChangeVersionHash) -> Result<OwnersTexts> {
        let mut chain: Vec<(ChangeVersionHash, ChangeVersion)> = Vec::new();
        let mut at = *version;
        let mut base = loop {
            if let Some(texts) = self
                .derive
                .get_keyed::<OwnersTexts>(OWNERS_KIND, &at)
                .await?
            {
                break Some(texts);
            }
            let cv = self.derive.change(&at).await?;
            let parent = cv.first_parent();
            chain.push((at, cv));
            match parent {
                Some(p) if chain.len() < MAX_CHAIN => at = p,
                _ => break None,
            }
        };
        if base.is_none() {
            // No cached ancestor: walk the oldest version reached, then
            // patch forward from it.
            let (oldest, _) = chain.pop().expect("the chain holds at least `version`");
            let texts = self.walk_owners(&oldest).await?;
            self.derive.put_keyed(OWNERS_KIND, &oldest, &texts).await?;
            base = Some(texts);
        }
        let mut texts = base.unwrap_or_default();
        for (hash, cv) in chain.into_iter().rev() {
            texts = self.patch_owners(texts, &hash, &cv).await?;
            self.derive.put_keyed(OWNERS_KIND, &hash, &texts).await?;
        }
        Ok(texts)
    }

    /// `texts` updated with what `cv` (stored as `hash`) did to owners files.
    async fn patch_owners(
        &self,
        mut texts: OwnersTexts,
        hash: &ChangeVersionHash,
        cv: &ChangeVersion,
    ) -> Result<OwnersTexts> {
        if !cv.subtree_changes.is_empty() {
            // A directory copy or delete may move any number of files.
            return self.walk_owners(hash).await;
        }
        for (path, change) in &cv.file_changes {
            if path.file_name() != Some(OWNERS_FILE) {
                continue;
            }
            let dir = path.parent().as_str().to_string();
            match change {
                FileChange::Set { content, .. } => {
                    let bytes =
                        rill_core::read_content(self.repo.objects().as_ref(), content).await?;
                    texts
                        .0
                        .insert(dir, String::from_utf8_lossy(&bytes).into_owned());
                }
                FileChange::Delete => {
                    texts.0.remove(&dir);
                }
                // A conflicted file keeps the parent's ownership until it
                // is resolved.
                FileChange::Conflict(_) => {}
            }
        }
        Ok(texts)
    }

    /// Every owners file in `version`'s tree, by walking its directories.
    async fn walk_owners(&self, version: &ChangeVersionHash) -> Result<OwnersTexts> {
        let root = self.derive.get::<RootTree>(version).await?;
        let trees = self.derive.trees();
        let mut texts = OwnersTexts::default();
        let mut pending = vec![RepoPathBuf::root()];
        while let Some(dir) = pending.pop() {
            for (name, entry) in trees.list_dir(root, &dir).await? {
                match entry {
                    EntryKind::Dir { .. } => pending.push(dir.join(&name)?),
                    EntryKind::File { content, .. } if name == OWNERS_FILE => {
                        let bytes =
                            rill_core::read_content(self.repo.objects().as_ref(), &content).await?;
                        texts.0.insert(
                            dir.as_str().to_string(),
                            String::from_utf8_lossy(&bytes).into_owned(),
                        );
                    }
                    EntryKind::File { .. } => {}
                }
            }
        }
        Ok(texts)
    }
}

#[cfg(test)]
mod tests {
    use rill_core::ChangeBuilder;

    use crate::testing::fixture;

    #[tokio::test]
    async fn the_index_follows_owners_files_through_history_like_a_full_walk() {
        let f = fixture().await;
        let (_, v1) = f
            .root_change("OWNERS.toml", "owners = [\"team:root\"]")
            .await;
        let (_, v2) = f
            .child_change(v1, "a/OWNERS.toml", "owners = [\"team:a\"]")
            .await;
        let (_, v3) = f.child_change(v2, "a/x.rs", "fn x() {}").await;
        let (v4, _) = ChangeBuilder::new(f.alice.clone())
            .parent(v3)
            .delete("a/OWNERS.toml")
            .unwrap()
            .commit(&f.repo)
            .await
            .unwrap();

        // Asking for the newest first patches forward from a walk of the
        // oldest; each step must equal walking that version directly.
        let newest = f.service.owners_texts(&v4).await.unwrap();
        assert_eq!(newest.0.keys().collect::<Vec<_>>(), [""]);
        for v in [v1, v2, v3, v4] {
            assert_eq!(
                f.service.owners_texts(&v).await.unwrap(),
                f.service.walk_owners(&v).await.unwrap(),
                "{v}"
            );
        }
        let at_v3 = f.service.owners_texts(&v3).await.unwrap();
        assert_eq!(at_v3.0["a"], "owners = [\"team:a\"]");
    }
}
