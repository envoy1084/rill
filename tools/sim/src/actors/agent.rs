//! Simulated agents and the changes they own.

use rill_types::{ChangeId, Principal};

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
