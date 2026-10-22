//! The self-healing SLOs a trunk-red run is held to.

use std::time::Duration;

/// How fast a break must be seen, blamed and repaired.
#[derive(Debug, Clone, Copy)]
pub struct Slo {
    /// From landing to trunk showing red.
    pub detect: Duration,
    /// From landing to an incident naming the culprit.
    pub attribute: Duration,
    /// The server's wait for a forward fix before reverting.
    pub grace: Duration,
    /// From attribution, past `grace`, to the culprit reverted (or its
    /// blocked revert recorded).
    pub revert: Duration,
}

impl Slo {
    /// The phase 18 targets for a server reverting after `grace`.
    pub fn with_grace(grace: Duration) -> Self {
        Slo {
            detect: Duration::from_secs(120),
            attribute: Duration::from_secs(300),
            grace,
            revert: Duration::from_secs(120),
        }
    }
}
