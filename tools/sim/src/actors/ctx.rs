//! What an action may touch, dispatch to each action, and the lookups
//! actions share.

use rill_types::{ChangeId, ChangeVersionHash, Principal, Timestamp};

use super::{Action, Agent, Outcome, Owned};
use crate::oracle::Oracle;
use crate::rng::SimRng;
use crate::world::World;

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
    pub(super) fn now(&self) -> Timestamp {
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
            Action::KillAgent => self.kill_agent(agent).await,
        }
    }

    pub(super) async fn current(&self, id: &ChangeId) -> Option<ChangeVersionHash> {
        self.world
            .repo
            .change_versions(id)
            .await
            .ok()?
            .last()
            .copied()
    }

    pub(super) fn pick_own(&mut self, agent: &Agent, submitted: Option<bool>) -> Option<Owned> {
        let candidates: Vec<&Owned> = agent
            .changes
            .iter()
            .filter(|c| submitted.is_none_or(|s| c.submitted == s))
            .collect();
        self.rng.pick(&candidates).map(|c| (*c).clone())
    }
}
