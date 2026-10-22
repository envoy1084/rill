//! Deciding what lands together and in which order: conflict domains,
//! fair order across lanes and sponsors, ordered groups, the speculation plan, swarm
//! throttling and ETAs. Pure logic; the sequencer does the I/O.

mod eta;
mod fair;
mod footprint;
mod groups;
mod plan;
mod rules;
mod thrash;

pub use eta::{Eta, Timings};
pub use fair::{serve_order, share_key};
pub use footprint::Footprint;
pub use groups::{
    group_domain, group_failure, group_of, hold_split_groups, keep_groups_together, window_len,
};
pub use plan::{Plan, Slot};
pub use rules::{LandingRules, ThrottleRules, RULES_PATH};
pub use thrash::Thrash;
