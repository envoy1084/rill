//! Shared review fixture: a service over in-memory stores.

#![allow(dead_code)]

use std::sync::Arc;

use rill_core::{ChangeBuilder, CommitGraph, DeriveManager, TreeOps};
use rill_policy::{RiskConfig, SubmitPolicy};
use rill_review::{LabelSet, ReviewService};
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeId, ChangeVersionHash, Principal};

pub struct Harness {
    pub repo: RepoStore,
    pub service: ReviewService,
    pub alice: Principal,
    pub bob: Principal,
    pub agent_a: Principal,
    pub agent_b: Principal,
}

pub async fn harness() -> Harness {
    harness_on(Stores::memory()).await
}

/// A harness over `stores`.
pub async fn harness_on(stores: Stores) -> Harness {
    let config = stores.create_repo("flow", "").await.unwrap();
    let repo = stores.repo(config.id);
    let trees = TreeOps::new(repo.objects().clone(), 64);
    let derive = DeriveManager::new(repo.clone(), trees, 64);
    let graph = Arc::new(CommitGraph::new(repo.clone(), derive.clone()));
    let service = ReviewService::new(
        repo.clone(),
        derive,
        graph,
        Arc::new(SubmitPolicy::default_rules()),
        Arc::new(RiskConfig::default().compile().unwrap()),
        Arc::new(LabelSet::default()),
    );
    Harness {
        repo,
        service,
        alice: Principal::human("alice"),
        bob: Principal::human("bob"),
        agent_a: Principal::agent("reviewer-a"),
        agent_b: Principal::agent("reviewer-b"),
    }
}

pub const LIB_V0: &str = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
pub const LIB_V1: &str =
    "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n";

impl Harness {
    pub async fn root(&self) -> ChangeVersionHash {
        ChangeBuilder::new(self.alice.clone())
            .message("root")
            .file("src/lib.rs", LIB_V0)
            .unwrap()
            .file("README.md", "# demo\n")
            .unwrap()
            .commit(&self.repo)
            .await
            .unwrap()
            .0
    }

    pub async fn child(
        &self,
        author: &Principal,
        parent: ChangeVersionHash,
        files: &[(&str, &str)],
    ) -> (ChangeId, ChangeVersionHash) {
        let mut b = ChangeBuilder::new(author.clone())
            .parent(parent)
            .message("child");
        for (p, c) in files {
            b = b.file(p, c.to_string()).unwrap();
        }
        let (h, cv) = b.commit(&self.repo).await.unwrap();
        (cv.change_id, h)
    }

    pub async fn amend(
        &self,
        change_id: ChangeId,
        predecessor: ChangeVersionHash,
        parent: ChangeVersionHash,
        files: &[(&str, &str)],
        message: &str,
    ) -> ChangeVersionHash {
        let mut b = ChangeBuilder::new(self.alice.clone())
            .change_id(change_id)
            .parent(parent)
            .predecessor(predecessor)
            .message(message);
        for (p, c) in files {
            b = b.file(p, c.to_string()).unwrap();
        }
        b.commit(&self.repo).await.unwrap().0
    }
}
