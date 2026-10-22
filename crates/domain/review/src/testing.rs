//! Shared fixture for unit tests.

use std::sync::Arc;

use rill_core::{ChangeBuilder, CommitGraph, DeriveManager, TreeOps};
use rill_policy::{RiskConfig, SubmitPolicy};
use rill_store::{RepoStore, Stores};
use rill_types::{ChangeId, ChangeVersionHash, Principal};

use crate::service::ReviewService;
use crate::votes::LabelSet;

/// In-memory repository with a review service over it.
pub struct Fixture {
    pub repo: RepoStore,
    pub service: ReviewService,
    pub alice: Principal,
    pub bot: Principal,
}

/// Build a fixture with default policy, risk config and labels.
pub async fn fixture() -> Fixture {
    let stores = Stores::memory();
    let config = stores.create_repo("test", "").await.unwrap();
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
    Fixture {
        repo,
        service,
        alice: Principal::human("alice"),
        bot: Principal::agent("bot"),
    }
}

impl Fixture {
    /// A parentless change setting one file.
    pub async fn root_change(&self, path: &str, content: &str) -> (ChangeId, ChangeVersionHash) {
        let (hash, cv) = ChangeBuilder::new(self.alice.clone())
            .message("root")
            .file(path, content.to_string())
            .unwrap()
            .commit(&self.repo)
            .await
            .unwrap();
        (cv.change_id, hash)
    }

    /// A new change on `parent` setting one file.
    pub async fn child_change(
        &self,
        parent: ChangeVersionHash,
        path: &str,
        content: &str,
    ) -> (ChangeId, ChangeVersionHash) {
        let (hash, cv) = ChangeBuilder::new(self.alice.clone())
            .parent(parent)
            .message(format!("edit {path}"))
            .file(path, content.to_string())
            .unwrap()
            .commit(&self.repo)
            .await
            .unwrap();
        (cv.change_id, hash)
    }

    /// A new version of `change_id` on `parent`, amending `predecessor`.
    pub async fn amend(
        &self,
        change_id: &ChangeId,
        predecessor: ChangeVersionHash,
        parent: ChangeVersionHash,
        path: &str,
        content: &str,
        message: &str,
    ) -> ChangeVersionHash {
        let (hash, _) = ChangeBuilder::new(self.alice.clone())
            .change_id(*change_id)
            .parent(parent)
            .predecessor(predecessor)
            .message(message)
            .file(path, content.to_string())
            .unwrap()
            .commit(&self.repo)
            .await
            .unwrap();
        hash
    }
}
