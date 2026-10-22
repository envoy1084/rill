//! A memory-backed repository with the services a lander needs.

use std::sync::Arc;

use rill_core::{read_content, ChangeBuilder, CommitGraph, DeriveManager, RootTree, TreeOps};
use rill_lander::{LandPolicy, Lander, ReviewHooks, Verifier};
use rill_store::{RepoStore, Stores};
use rill_types::{
    ChangeVersion, ChangeVersionHash, EntryKind, Principal, RepoPathBuf, TreeHash, TrunkSeq,
};

pub struct Fixture {
    pub repo: RepoStore,
    pub derive: Arc<DeriveManager>,
    pub trees: TreeOps,
}

impl Fixture {
    pub async fn new() -> Self {
        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        let repo = stores.repo(config.id);
        let trees = TreeOps::new(repo.objects().clone(), 64);
        let derive = DeriveManager::new(repo.clone(), trees.clone(), 64);
        Fixture {
            repo,
            derive,
            trees,
        }
    }

    pub fn lander(
        &self,
        hooks: Arc<dyn ReviewHooks>,
        verifier: Arc<dyn Verifier>,
        policy: LandPolicy,
    ) -> Arc<Lander> {
        Arc::new(self.bare_lander(hooks, verifier, policy))
    }

    /// A lander to add a regenerator or resolver to.
    pub fn bare_lander(
        &self,
        hooks: Arc<dyn ReviewHooks>,
        verifier: Arc<dyn Verifier>,
        policy: LandPolicy,
    ) -> Lander {
        let graph = Arc::new(CommitGraph::new(self.repo.clone(), self.derive.clone()));
        Lander::new(
            self.repo.clone(),
            self.derive.clone(),
            graph,
            hooks,
            verifier,
            policy,
        )
    }

    pub async fn commit(
        &self,
        parent: Option<ChangeVersionHash>,
        files: &[(&str, &str)],
        message: &str,
    ) -> (ChangeVersionHash, ChangeVersion) {
        let mut builder = ChangeBuilder::new(Principal::human("alice")).message(message);
        if let Some(parent) = parent {
            builder = builder.parent(parent);
        }
        for (path, content) in files {
            builder = builder.file(path, content.as_bytes().to_vec()).unwrap();
        }
        builder.commit(&self.repo).await.unwrap()
    }

    pub async fn delete(
        &self,
        parent: ChangeVersionHash,
        path: &str,
        message: &str,
    ) -> (ChangeVersionHash, ChangeVersion) {
        ChangeBuilder::new(Principal::human("alice"))
            .message(message)
            .parent(parent)
            .delete(path)
            .unwrap()
            .commit(&self.repo)
            .await
            .unwrap()
    }

    pub async fn root(&self, version: &ChangeVersionHash) -> TreeHash {
        self.derive.get::<RootTree>(version).await.unwrap()
    }

    pub async fn head(&self) -> (TrunkSeq, ChangeVersionHash) {
        self.repo.trunk_head().await.unwrap().unwrap()
    }

    pub async fn file(&self, root: TreeHash, path: &str) -> Option<String> {
        match self
            .trees
            .lookup(root, &RepoPathBuf::new(path).unwrap())
            .await
            .unwrap()
        {
            Some(EntryKind::File { content, .. }) => {
                let bytes = read_content(self.trees.objects().as_ref(), &content)
                    .await
                    .unwrap();
                Some(String::from_utf8(bytes.to_vec()).unwrap())
            }
            _ => None,
        }
    }
}
