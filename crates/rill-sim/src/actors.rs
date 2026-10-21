//! Simulated agents and reviewers, and the actions they take.

use std::sync::Arc;

use rill_core::{rebase, ChangeBuilder};
use rill_lander::LandRequest;
use rill_types::{ChangeId, ChangeVersionHash, Principal, RepoPathBuf, ReviewStatus, Timestamp};

use crate::oracle::{Invariant, Oracle};
use crate::rng::SimRng;
use crate::world::World;

/// Files every change edits, plus one hot file most changes collide on.
pub const FILES: &[&str] = &["src/a.rs", "src/b.rs", "src/c.rs", "docs/notes.md"];
pub const HOT_FILE: &str = "Cargo.lock";
/// Lines in each file at the start.
pub const FILE_LINES: usize = 8;

/// Something an actor does in one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NewChange,
    Amend,
    Submit,
    Vote,
    RequestLand,
    Sweep,
    Sync,
    Abandon,
    DeriveWork,
    ZombieLand,
    RivalSweep,
    Partition,
}

impl Action {
    const WEIGHTED: &'static [(Action, usize)] = &[
        (Action::NewChange, 20),
        (Action::Amend, 8),
        (Action::Submit, 14),
        (Action::Vote, 20),
        (Action::RequestLand, 14),
        (Action::Sweep, 12),
        (Action::Sync, 6),
        (Action::Abandon, 3),
        (Action::DeriveWork, 8),
        (Action::ZombieLand, 3),
        (Action::RivalSweep, 6),
        (Action::Partition, 1),
    ];

    /// Draw an action by weight.
    pub fn draw(rng: &mut SimRng) -> Action {
        let total: usize = Self::WEIGHTED.iter().map(|(_, w)| w).sum();
        let mut roll = rng.below(total);
        for (action, weight) in Self::WEIGHTED {
            if roll < *weight {
                return *action;
            }
            roll -= weight;
        }
        unreachable!("roll is below the total weight")
    }
}

/// How an action ended, for the run trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Done,
    Skipped,
    Failed,
}

/// An agent that authors changes.
#[derive(Debug)]
pub struct Agent {
    pub principal: Principal,
    pub changes: Vec<Owned>,
}

/// A change an agent owns.
#[derive(Debug, Clone)]
pub struct Owned {
    pub id: ChangeId,
    pub submitted: bool,
}

/// Everything an action may touch.
pub struct Ctx<'a> {
    pub world: &'a World,
    pub oracle: &'a mut Oracle,
    pub rng: &'a mut SimRng,
    pub reviewers: &'a [Principal],
    pub hot_file_share: f64,
    pub stale_base_share: f64,
    pub partitions: bool,
    pub step: usize,
}

impl Ctx<'_> {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(crate::run::START_MILLIS + self.step as i64 * 1_000)
    }

    /// Run `action` for `agent`.
    pub async fn act(&mut self, agent: &mut Agent, action: Action) -> Outcome {
        match action {
            Action::NewChange => self.new_change(agent).await,
            Action::Amend => self.amend(agent).await,
            Action::Submit => self.submit(agent).await,
            Action::Vote => self.vote(agent).await,
            Action::RequestLand => self.request_land(agent).await,
            Action::Sweep => self.sweep().await,
            Action::Sync => self.sync(agent).await,
            Action::Abandon => self.abandon(agent).await,
            Action::DeriveWork => self.derive_work().await,
            Action::ZombieLand => self.zombie_land().await,
            Action::RivalSweep => self.rival_sweep().await,
            Action::Partition => self.partition().await,
        }
    }

    async fn base(&mut self) -> Option<ChangeVersionHash> {
        let head = self.world.repo.trunk_head().await.ok()??;
        let (seq, hash) = head;
        if seq.get() > 1 && self.rng.chance(self.stale_base_share) {
            let older = 1 + self.rng.below(seq.get() as usize - 1) as u64;
            return self
                .world
                .repo
                .trunk_at(rill_types::TrunkSeq(older))
                .await
                .ok()
                .flatten();
        }
        Some(hash)
    }

    fn path(&mut self) -> RepoPathBuf {
        let name = if self.rng.chance(self.hot_file_share) {
            HOT_FILE
        } else {
            FILES[self.rng.below(FILES.len())]
        };
        RepoPathBuf::new(name).expect("fixed paths are valid")
    }

    /// `path` at `base` with one line replaced by a line unique to this
    /// step, so concurrent edits sometimes overlap and sometimes do not.
    async fn edited(
        &mut self,
        base: ChangeVersionHash,
        path: &RepoPathBuf,
        who: &Principal,
    ) -> Option<String> {
        let mut lines = self.world.file_lines(base, path).await.ok()?;
        let line = format!("{} step {}", who.name, self.step);
        if lines.is_empty() || self.rng.chance(0.1) {
            lines.push(line);
        } else {
            let at = self.rng.below(lines.len());
            lines[at] = line;
        }
        Some(lines.join("\n") + "\n")
    }

    async fn new_change(&mut self, agent: &mut Agent) -> Outcome {
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

    async fn current(&self, id: &ChangeId) -> Option<ChangeVersionHash> {
        self.world
            .repo
            .change_versions(id)
            .await
            .ok()?
            .last()
            .copied()
    }

    fn pick_own(&mut self, agent: &Agent, submitted: Option<bool>) -> Option<Owned> {
        let candidates: Vec<&Owned> = agent
            .changes
            .iter()
            .filter(|c| submitted.is_none_or(|s| c.submitted == s))
            .collect();
        self.rng.pick(&candidates).map(|c| (*c).clone())
    }

    async fn amend(&mut self, agent: &mut Agent) -> Outcome {
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

    async fn submit(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, Some(false)) else {
            return Outcome::Skipped;
        };
        let submitted = self
            .world
            .review()
            .submit(&owned.id, &agent.principal)
            .await;
        // A lost acknowledgement may have stored the review anyway.
        let exists = matches!(self.world.repo.get_review(&owned.id).await, Ok(Some(_)));
        if submitted.is_ok() || exists {
            if let Some(c) = agent.changes.iter_mut().find(|c| c.id == owned.id) {
                c.submitted = true;
            }
        }
        if submitted.is_ok() {
            Outcome::Done
        } else {
            Outcome::Failed
        }
    }

    async fn vote(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, Some(true)) else {
            return Outcome::Skipped;
        };
        let Some(voter) = self.rng.pick(self.reviewers).cloned() else {
            return Outcome::Skipped;
        };
        let (label, value) = match self.rng.below(20) {
            0 => ("Review", -2),
            1..=4 => ("Verified", 1),
            _ => ("Review", 2),
        };
        match self
            .world
            .review()
            .vote(&owned.id, &voter, label, value, None)
            .await
        {
            Ok(_) => Outcome::Done,
            Err(_) => Outcome::Failed,
        }
    }

    async fn request_land(&mut self, agent: &mut Agent) -> Outcome {
        let Some(owned) = self.pick_own(agent, Some(true)) else {
            return Outcome::Skipped;
        };
        let review = self.world.review();
        let Ok(state) = review.get(&owned.id).await else {
            return Outcome::Failed;
        };
        if state.status != ReviewStatus::Landable {
            return Outcome::Skipped;
        }
        let Ok(decision) = review.evaluate_for(&state, &agent.principal).await else {
            return Outcome::Failed;
        };
        let Some(version) = state.current_version() else {
            return Outcome::Skipped;
        };
        if !decision.allowed {
            return Outcome::Skipped;
        }
        let request = LandRequest::new(owned.id, version, agent.principal.clone(), state.risk);
        match self.world.lander.enqueue(request).await {
            Ok(ticket) => {
                self.oracle.requested(owned.id, ticket);
                Outcome::Done
            }
            Err(_) => Outcome::Failed,
        }
    }

    async fn sweep(&mut self) -> Outcome {
        let lander = Arc::clone(&self.world.lander);
        let outcome = match lander.run_once().await {
            Ok(_) => Outcome::Done,
            Err(_) => Outcome::Failed,
        };
        self.oracle.observe_trunk(&self.world.repo).await;
        outcome
    }

    async fn sync(&mut self, agent: &mut Agent) -> Outcome {
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

    async fn abandon(&mut self, agent: &mut Agent) -> Outcome {
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
