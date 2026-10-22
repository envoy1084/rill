//! Agents and the kill switch: every simulated agent is registered,
//! sponsored by one human, and an action suspends one at random. Once
//! suspended, nothing it wrote may land.

use rill_types::identity::{
    AccountStatus, AgentBudgets, AgentRecord, IdentityChange, Sponsor, UserRecord,
};
use rill_types::{Principal, Timestamp, TrunkSeq};

use crate::actors::{Agent, Ctx, Outcome};
use crate::oracle::{Invariant, Oracle};
use crate::world::World;

/// The human who sponsors every simulated agent.
const SPONSOR: &str = "sponsor";

/// Register `agents` in the world's org, sponsored by one active user.
pub async fn register_agents(world: &World, agents: &[Agent]) {
    let store = world.identity.store();
    let admin = Principal::service("sim");
    let now = Timestamp::now();
    store
        .create_user(
            &UserRecord {
                org: world.org.clone(),
                principal: Principal::human(SPONSOR),
                email: None,
                display_name: None,
                external: vec![],
                status: AccountStatus::Active,
                roles: vec![],
                teams: vec![],
                scim_id: None,
                external_id: None,
                created_at: now,
                updated_at: now,
            },
            &admin,
        )
        .await
        .expect("identity has no faults");
    for agent in agents {
        store
            .create_agent(
                &AgentRecord {
                    org: world.org.clone(),
                    principal: agent.principal.clone(),
                    sponsor: Sponsor::User(SPONSOR.into()),
                    purpose: "simulation".into(),
                    repos: vec!["*".into()],
                    paths: vec![],
                    models: vec![],
                    budgets: AgentBudgets::default(),
                    workloads: vec![],
                    status: AccountStatus::Active,
                    expires_at: None,
                    created_by: admin.clone(),
                    created_at: now,
                    updated_at: now,
                },
                &admin,
            )
            .await
            .expect("identity has no faults");
    }
}

impl Ctx<'_> {
    /// Suspend `agent`, as the kill switch does, and tell the identity view
    /// as the server's follower would.
    pub(crate) async fn kill_agent(&mut self, agent: &Agent) -> Outcome {
        let store = self.world.identity.store();
        let name = &agent.principal.name;
        let Ok(Some(before)) = store.agent(&self.world.org, name).await else {
            return Outcome::Failed;
        };
        if before.status == AccountStatus::Suspended {
            return Outcome::Skipped;
        }
        let after = AgentRecord {
            status: AccountStatus::Suspended,
            updated_at: Timestamp::now(),
            ..before.clone()
        };
        if store
            .update_agent(&before, &after, &Principal::service("sim"))
            .await
            .is_err()
        {
            return Outcome::Failed;
        }
        self.world.identity.apply(&IdentityChange::AgentChanged {
            org: self.world.org.clone(),
            name: name.clone(),
        });
        // What trunk holds at the kill, read without faults: a failed read
        // would date earlier landings after it.
        let Ok(head) = self.world.truth.trunk_head().await else {
            return Outcome::Failed;
        };
        let len = head.map_or(0, |(seq, _)| seq.get() as usize);
        self.oracle.killed(name.clone(), len);
        Outcome::Done
    }
}

impl Oracle {
    /// No trunk position after an agent's suspension holds a version it
    /// wrote.
    pub async fn check_killed_agents(&mut self, world: &World) {
        let kills: Vec<(String, usize)> = self.kills().to_vec();
        for (name, trunk_len) in kills {
            for seq in (trunk_len + 1)..=self.trunk_len() {
                let Ok(Some(version)) = world.truth.trunk_at(TrunkSeq(seq as u64)).await else {
                    continue;
                };
                let Ok(Some(cv)) = world.truth.get_change_version(&version).await else {
                    continue;
                };
                if cv.author.name == name {
                    self.violate(
                        Invariant::KilledAgentsDoNotLand,
                        format!("trunk@{seq} is {name}'s, landed after {name} was suspended"),
                    );
                }
            }
        }
    }
}
