//! Which agent approvals are independent of a change's author.
//!
//! An agent's positive vote counts toward a quorum when the agent is not
//! the author, its sponsor is neither the author nor the author's sponsor
//! (so an agent swarm cannot approve its own work), and no reviewer already
//! counted runs the same model family and prompt. Agents without a
//! reviewer profile count as their own kind. A demoted reviewer never
//! counts.

use std::collections::{BTreeMap, BTreeSet};

use rill_types::{LabelVote, Principal, PrincipalKind};
use serde::{Deserialize, Serialize};

use crate::directory::Person;

/// Why each approving agent did or did not count.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Independence {
    /// Agents whose approvals count.
    pub counted: Vec<String>,
    /// Agents whose approvals do not, with the reason.
    pub excluded: Vec<(String, String)>,
    /// The lowest trust (permille) among the counted; 0 when none.
    pub min_trust: u16,
}

impl Independence {
    pub fn count(&self) -> u32 {
        self.counted.len() as u32
    }
}

/// Trust of an agent with no record yet: the prior's mean.
const UNKNOWN_TRUST: u16 = 500;

fn sponsor_key(principal: &Principal) -> String {
    format!("user:{}", principal.name)
}

/// Sort the positive agent votes in `votes` into independent and not,
/// against `author`, with `people` keyed by display form.
pub fn independent_approvals<'a>(
    votes: impl IntoIterator<Item = &'a LabelVote>,
    author: &Principal,
    people: &BTreeMap<String, Person>,
) -> Independence {
    let mut latest: BTreeMap<_, &LabelVote> = BTreeMap::new();
    for v in votes {
        latest.insert(v.voter.id, v);
    }
    let person = |p: &Principal| people.get(&p.to_string()).cloned().unwrap_or_default();
    let author_person = person(author);
    let mut tainted: BTreeSet<String> = author_person.sponsor.into_iter().collect();
    if author.kind == PrincipalKind::Human {
        tainted.insert(sponsor_key(author));
    }
    let mut kinds = BTreeSet::new();
    let mut out = Independence::default();
    let mut agents: Vec<&LabelVote> = latest
        .into_values()
        .filter(|v| v.voter.kind == PrincipalKind::Agent && v.value > 0)
        .collect();
    agents.sort_by_key(|v| v.voted_at);
    for v in agents {
        let name = v.voter.to_string();
        let voter = person(&v.voter);
        let reason = if v.voter.id == author.id {
            Some("is the author".to_string())
        } else if voter.demoted {
            Some("is demoted for its track record".to_string())
        } else if let Some(s) = voter.sponsor.as_ref().filter(|s| tainted.contains(*s)) {
            Some(format!("shares the author's sponsor chain ({s})"))
        } else {
            let kind = match &voter.reviewer {
                Some(r) => (r.model_family.clone(), r.prompt.clone()),
                None => (name.clone(), String::new()),
            };
            (!kinds.insert(kind.clone())).then(|| {
                format!(
                    "runs the same model family and prompt as another ({})",
                    kind.0
                )
            })
        };
        match reason {
            Some(r) => out.excluded.push((name, r)),
            None => {
                let trust = voter.trust.unwrap_or(UNKNOWN_TRUST);
                out.min_trust = if out.counted.is_empty() {
                    trust
                } else {
                    out.min_trust.min(trust)
                };
                out.counted.push(name);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use rill_types::identity::ReviewerProfile;
    use rill_types::{ObjectHash, ObjectKind, Timestamp};

    use super::*;

    fn vote(who: &Principal, value: i8, at: i64) -> LabelVote {
        LabelVote {
            label: "Review".into(),
            value,
            voter: who.clone(),
            version: ObjectHash::digest(ObjectKind::Derived, b"v"),
            voted_at: Timestamp::from_millis(at),
            reason: None,
        }
    }

    fn reviewer(sponsor: &str, family: &str, prompt: &str) -> Person {
        Person {
            sponsor: Some(sponsor.into()),
            reviewer: Some(ReviewerProfile {
                model_family: family.into(),
                prompt: prompt.into(),
                ..ReviewerProfile::default()
            }),
            ..Person::default()
        }
    }

    #[test]
    fn a_swarm_never_approves_its_own_work() {
        let author = Principal::agent("coder");
        let (a, b, c, d, e) = (
            Principal::agent("sec"),
            Principal::agent("perf"),
            Principal::agent("sec-twin"),
            Principal::agent("buddy"),
            Principal::agent("style"),
        );
        let people = BTreeMap::from([
            (
                author.to_string(),
                Person {
                    sponsor: Some("user:ann".into()),
                    ..Person::default()
                },
            ),
            (a.to_string(), reviewer("team:review", "claude", "sec.md")),
            (b.to_string(), reviewer("team:review", "gpt", "perf.md")),
            (c.to_string(), reviewer("team:review", "claude", "sec.md")),
            (d.to_string(), reviewer("user:ann", "gemini", "x.md")),
            (e.to_string(), reviewer("team:review", "claude", "style.md")),
        ]);
        let votes = [
            vote(&a, 1, 1),
            vote(&b, 1, 2),
            vote(&c, 1, 3),
            vote(&d, 1, 4),
            vote(&e, -1, 5),
            vote(&author, 1, 6),
        ];
        let got = independent_approvals(&votes, &author, &people);
        assert_eq!(got.counted, ["agent:sec", "agent:perf"]);
        assert_eq!(got.excluded.len(), 3, "{:?}", got.excluded);
        assert_eq!(got.min_trust, 500, "no records yet");

        let mut people = people;
        people.get_mut(&a.to_string()).unwrap().demoted = true;
        people.get_mut(&b.to_string()).unwrap().trust = Some(910);
        let got = independent_approvals(&votes, &author, &people);
        assert_eq!(
            got.counted,
            ["agent:perf", "agent:sec-twin"],
            "demoted sec no longer counts, so its twin does"
        );
        assert_eq!(got.min_trust, 500);

        let human = Principal::human("ann");
        let got = independent_approvals(&votes, &human, &people);
        assert!(got.counted.contains(&"agent:perf".to_string()));
        assert!(
            !got.counted.contains(&"agent:buddy".to_string()),
            "ann's own agent"
        );
    }
}
