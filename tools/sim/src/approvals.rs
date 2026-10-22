//! Invariant 2: every landing of a reviewed change was allowed by the
//! policy recorded on it, judged on the votes recorded with it.

use rill_policy::SubmitPolicy;
use rill_types::TrunkSeq;

use crate::oracle::{Invariant, Oracle};
use crate::world::World;

impl Oracle {
    /// Re-judge every landing of a reviewed change with the votes its
    /// landing record says the policy saw.
    pub async fn check_landings_allowed(&mut self, world: &World) {
        let digest = SubmitPolicy::default_rules().digest();
        let review = world.review();
        for seq in 1..=self.trunk_len() as u64 {
            let Ok(Some(landing)) = world.repo.landing(TrunkSeq(seq)).await else {
                continue;
            };
            let Ok(Some(mut state)) = world.repo.get_review(&landing.change_id).await else {
                continue;
            };
            if landing.policy != Some(digest) {
                self.violate(
                    Invariant::LandingsWereAllowed,
                    format!("trunk@{seq} records policy {:?}", landing.policy.map(hex)),
                );
                continue;
            }
            // Judge the landed version on the recorded votes alone, whatever
            // happened to the review since.
            let Some(current) = state.current_version() else {
                continue;
            };
            state.votes = landing
                .approvals
                .iter()
                .cloned()
                .map(|mut v| {
                    v.version = current;
                    v
                })
                .collect();
            match review.evaluate_for(&state, &landing.requester).await {
                Ok(decision) if decision.allowed => {}
                Ok(decision) => self.violate(
                    Invariant::LandingsWereAllowed,
                    format!(
                        "trunk@{seq} landed on votes the policy refuses: {}",
                        decision.reasons.join("; ")
                    ),
                ),
                Err(_) => {}
            }
        }
    }
}

fn hex(digest: [u8; 32]) -> String {
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}
