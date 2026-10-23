//! Teams, sponsors and reviewer profiles for review, from the org's identity records.

use std::sync::Arc;

use async_trait::async_trait;
use rill_policy::{Directory, Person};
use rill_types::{Principal, PrincipalKind};

use super::view::IdentityView;

/// Answers [`Directory`] questions for one org: a user's teams, an
/// agent's sponsor and reviewer profile.
pub struct OrgDirectory {
    org: String,
    identity: Arc<IdentityView>,
}

impl OrgDirectory {
    pub fn new(org: impl Into<String>, identity: Arc<IdentityView>) -> Self {
        OrgDirectory {
            org: org.into(),
            identity,
        }
    }
}

#[async_trait]
impl Directory for OrgDirectory {
    async fn person(&self, principal: &Principal) -> rill_policy::Result<Person> {
        let lookup = |e: crate::error::Error| rill_policy::Error::PolicyEvaluation(e.to_string());
        Ok(match principal.kind {
            PrincipalKind::Human => Person {
                teams: self
                    .identity
                    .user(&self.org, &principal.name)
                    .await
                    .map_err(lookup)?
                    .map(|u| u.teams.clone())
                    .unwrap_or_default(),
                ..Person::default()
            },
            PrincipalKind::Agent => {
                let agent = self
                    .identity
                    .agent(&self.org, &principal.name)
                    .await
                    .map_err(lookup)?;
                Person {
                    teams: Vec::new(),
                    sponsor: agent.as_ref().map(|a| a.sponsor.label()),
                    reviewer: agent.and_then(|a| a.reviewer.clone()),
                    ..Person::default()
                }
            }
            PrincipalKind::Service => Person::default(),
        })
    }
}
