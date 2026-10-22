//! Judging trunk's health from its open incidents.

use std::time::Duration;

use rill_types::{HealthState, Incident, IncidentState, Timestamp, TrunkHealth};

/// When red becomes guarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardRules {
    /// Red this long.
    pub after: Duration,
    /// This many incidents open at once.
    pub incidents: usize,
}

/// Trunk's health given its `open` incidents at `now`, keeping what an
/// administrator set (the freeze) and when it went red.
pub fn judge(
    previous: &TrunkHealth,
    open: &[Incident],
    now: Timestamp,
    rules: GuardRules,
) -> TrunkHealth {
    let state = if open.is_empty() {
        HealthState::Green
    } else if open.iter().all(|i| i.state == IncidentState::Fixing) {
        HealthState::Fixing
    } else {
        HealthState::Red
    };
    let was_green = previous.state == HealthState::Green;
    let since = match (state, was_green) {
        (HealthState::Green, true) => previous.since,
        (HealthState::Green, false) => now,
        (_, true) => now,
        (_, false) => previous.since,
    };
    let red_for = Duration::from_millis((now.millis() - since.millis()).max(0) as u64);
    let state = match state {
        HealthState::Red if red_for >= rules.after || open.len() >= rules.incidents => {
            HealthState::Guarded
        }
        other => other,
    };
    let mut failing_steps: Vec<String> = open.iter().flat_map(|i| i.steps.clone()).collect();
    failing_steps.sort();
    failing_steps.dedup();
    TrunkHealth {
        state,
        since,
        open: open.iter().map(|i| i.culprit).collect(),
        failing_steps,
        frozen: previous.frozen,
        frozen_reason: previous.frozen_reason.clone(),
    }
}

#[cfg(test)]
mod tests {
    use rill_types::{ChangeId, ObjectHash, Principal, TrunkSeq};

    use super::*;

    const RULES: GuardRules = GuardRules {
        after: Duration::from_secs(30 * 60),
        incidents: 3,
    };

    fn incident(culprit: u64, state: IncidentState) -> Incident {
        Incident {
            culprit: TrunkSeq(culprit),
            version: ObjectHash::ZERO,
            change: ChangeId::new(),
            failures: vec![],
            steps: vec![format!("step{culprit}")],
            details: String::new(),
            author: Principal::agent("bot"),
            session: None,
            model: None,
            sponsor: None,
            state,
            opened_at: Timestamp::from_millis(0),
            updated_at: Timestamp::from_millis(0),
            fix: None,
            resolution: None,
            revert_blocked: None,
            assigned: None,
        }
    }

    fn at(minutes: i64) -> Timestamp {
        Timestamp::from_millis(minutes * 60_000)
    }

    #[test]
    fn red_turns_guarded_after_half_an_hour_or_three_culprits() {
        let green = TrunkHealth::default();
        let red = judge(&green, &[incident(5, IncidentState::Open)], at(10), RULES);
        assert_eq!((red.state, red.since), (HealthState::Red, at(10)));
        let still = judge(&red, &[incident(5, IncidentState::Open)], at(39), RULES);
        assert_eq!(still.state, HealthState::Red);
        let long = judge(&still, &[incident(5, IncidentState::Open)], at(40), RULES);
        assert_eq!((long.state, long.since), (HealthState::Guarded, at(10)));

        let three: Vec<_> = (1..=3).map(|c| incident(c, IncidentState::Open)).collect();
        let many = judge(&green, &three, at(1), RULES);
        assert_eq!(many.state, HealthState::Guarded);
        assert_eq!(many.failing_steps, vec!["step1", "step2", "step3"]);
    }

    #[test]
    fn fixing_then_green_and_the_freeze_is_kept() {
        let frozen = TrunkHealth {
            frozen: true,
            ..TrunkHealth::default()
        };
        let red = judge(&frozen, &[incident(5, IncidentState::Open)], at(1), RULES);
        let fixing = judge(&red, &[incident(5, IncidentState::Fixing)], at(2), RULES);
        assert_eq!(fixing.state, HealthState::Fixing);
        let green = judge(&fixing, &[], at(3), RULES);
        assert_eq!(
            (green.state, green.since, green.frozen),
            (HealthState::Green, at(3), true)
        );
    }
}
