//! Agents writing changes: new, amended, rebased onto trunk and abandoned.

use rill_core::{rebase, ChangeBuilder};
use rill_types::ChangeId;

use super::{Agent, Ctx, Outcome, Owned};
use crate::oracle::Invariant;

impl Ctx<'_> {
    pub(super) async fn new_change(&mut self, agent: &mut Agent) -> Outcome {
        let Some(base) = self.base().await else {
            return Outcome::Failed;
        };
        let path = self.path();
        let Some(text) = self.edited(base, &path, &agent.principal).await else {
            return Outcome::Failed;
        };
        let id = ChangeId::from_bytes(self.rng.bytes16());
        let built = ChangeBuilder::new(agent.principal.clone())
            .change_id(id)
            .parent(base)
            .message(format!("edit {path}"))
            .created_at(self.now())
            .file(path.as_str(), text)
            .expect("fixed paths are valid")
            .commit(&self.world.repo)
            .await;
        match built {
            Ok((hash, _)) => {
                self.oracle.acknowledged(hash);
                agent.changes.push(Owned {
                    id,
                    submitted: false,
                });
                Outcome::Done
            }
            Err(_) => Outcome::Failed,
        }
    }

    pub(super) async fn amend(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, None) else {
            return Outcome::Skipped;
        };
        let Some(current) = self.current(&owned.id).await else {
            return Outcome::Failed;
        };
        let Ok(Some(cv)) = self.world.repo.get_change_version(&current).await else {
            return Outcome::Failed;
        };
        let Some(parent) = cv.first_parent() else {
            return Outcome::Skipped;
        };
        let path = self.path();
        let Some(text) = self.edited(current, &path, &agent.principal).await else {
            return Outcome::Failed;
        };
        let mut builder = ChangeBuilder::new(agent.principal.clone())
            .change_id(owned.id)
            .parent(parent)
            .predecessor(current)
            .message(cv.message.clone())
            .created_at(self.now());
        for (p, change) in &cv.file_changes {
            builder = builder
                .change(p.as_str(), change.clone())
                .expect("paths come from a stored version");
        }
        let builder = builder
            .file(path.as_str(), text)
            .expect("fixed paths are valid");
        let Ok((hash, _)) = builder.commit(&self.world.repo).await else {
            return Outcome::Failed;
        };
        self.oracle.acknowledged(hash);
        if owned.submitted
            && self
                .world
                .review()
                .add_version(&owned.id, hash)
                .await
                .is_err()
        {
            return Outcome::Failed;
        }
        Outcome::Done
    }

    pub(super) async fn sync(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, None) else {
            return Outcome::Skipped;
        };
        let (Some(current), Ok(Some((_, head)))) = (
            self.current(&owned.id).await,
            self.world.repo.trunk_head().await,
        ) else {
            return Outcome::Failed;
        };
        let rebased = match rebase(&self.world.derive, &current, &head, self.now()).await {
            Ok(cv) => cv,
            Err(e) if is_environmental(&e) => return Outcome::Failed,
            Err(e) => {
                self.oracle.violate(
                    Invariant::RebaseNeverFailsOnContent,
                    format!("rebase of {} failed: {e}", owned.id),
                );
                return Outcome::Failed;
            }
        };
        let Ok(hash) = self.world.repo.put_change_version(&rebased).await else {
            return Outcome::Failed;
        };
        self.oracle.acknowledged(hash);
        if owned.submitted
            && self
                .world
                .review()
                .add_version(&owned.id, hash)
                .await
                .is_err()
        {
            return Outcome::Failed;
        }
        Outcome::Done
    }

    pub(super) async fn abandon(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, Some(true)) else {
            return Outcome::Skipped;
        };
        match self
            .world
            .review()
            .abandon(&owned.id, &agent.principal)
            .await
        {
            Ok(_) => {
                self.oracle.abandoned(owned.id);
                Outcome::Done
            }
            Err(_) => Outcome::Failed,
        }
    }
}

/// Errors that come from the storage environment (injected faults,
/// corruption caught on read) rather than from the change's content.
fn is_environmental(e: &rill_core::Error) -> bool {
    let text = e.to_string();
    matches!(e, rill_core::Error::Store(_))
        || text.contains("injected fault")
        || text.contains("failed its hash check")
}
