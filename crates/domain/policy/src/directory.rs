//! Who people are, as review needs to know: their teams (for ownership)
//! and who sponsors an agent and how it reviews (for reviewer
//! independence).

use std::collections::BTreeMap;

use async_trait::async_trait;
use rill_types::identity::ReviewerProfile;
use rill_types::Principal;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// What review knows about one principal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub teams: Vec<String>,
    /// The sponsor of an agent (`user:<name>` or `team:<name>`).
    pub sponsor: Option<String>,
    /// How an agent reviews, when it is a registered reviewer.
    pub reviewer: Option<ReviewerProfile>,
    /// A reviewer's trust in this repository, in permille.
    pub trust: Option<u16>,
    /// A reviewer demoted for its track record.
    pub demoted: bool,
}

/// Looks up teams and sponsors. The server answers from its identity
/// store; tests and dev servers may know nobody.
#[async_trait]
pub trait Directory: Send + Sync {
    async fn person(&self, principal: &Principal) -> Result<Person>;
}

/// A directory that knows nobody: no teams, no sponsors.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyDirectory;

#[async_trait]
impl Directory for EmptyDirectory {
    async fn person(&self, _principal: &Principal) -> Result<Person> {
        Ok(Person::default())
    }
}

/// A fixed directory, for tests and simulations.
#[derive(Debug, Default, Clone)]
pub struct StaticDirectory(pub BTreeMap<String, Person>);

impl StaticDirectory {
    /// Add `principal` (by display form, `ann` or `agent:bot`).
    pub fn with(mut self, principal: &Principal, person: Person) -> Self {
        self.0.insert(principal.to_string(), person);
        self
    }
}

#[async_trait]
impl Directory for StaticDirectory {
    async fn person(&self, principal: &Principal) -> Result<Person> {
        Ok(self
            .0
            .get(&principal.to_string())
            .cloned()
            .unwrap_or_default())
    }
}
