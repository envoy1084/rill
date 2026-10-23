//! A memory-backed repository with derivation and a commit graph.

use std::sync::Arc;

use bytes::Bytes;
use rill_core::derive::RootTree;
use rill_core::{read_content, CommitGraph, DeriveManager, TreeOps};
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeVersionHash, EntryKind, Principal};

use super::values::p;

pub struct Fixture {
    pub repo: RepoStore,
    pub derive: Arc<DeriveManager>,
    pub graph: CommitGraph,
    pub alice: Principal,
    pub bob: Principal,
}

pub async fn fixture() -> Fixture {
    let stores = Stores::memory();
    let config = stores.create_repo("demo", "").await.unwrap();
    let repo = stores.repo(config.id);
    let trees = TreeOps::new(stores.objects.clone(), 256);
    let derive = DeriveManager::new(repo.clone(), trees, 256);
    let graph = CommitGraph::new(repo.clone(), derive.clone());
    Fixture {
        repo,
        derive,
        graph,
        alice: Principal::human("alice"),
        bob: Principal::agent("bob"),
    }
}

pub async fn content_at(f: &Fixture, version: &ChangeVersionHash, path: &str) -> Option<Bytes> {
    let root = f.derive.get::<RootTree>(version).await.unwrap();
    match f.derive.trees().lookup(root, &p(path)).await.unwrap() {
        Some(EntryKind::File { content, .. }) => Some(
            read_content(f.repo.objects().as_ref(), &content)
                .await
                .unwrap(),
        ),
        _ => None,
    }
}
