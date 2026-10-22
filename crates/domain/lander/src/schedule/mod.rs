//! Deciding what lands together and in which order: conflict domains,
//! fair order across lanes and sponsors, the speculation plan, swarm
//! throttling and ETAs. Pure logic; the sequencer does the I/O.

mod eta;
mod fair;
mod footprint;
mod plan;
mod rules;
mod thrash;

pub use eta::{Eta, Timings};
pub use fair::{serve_order, share_key};
pub use footprint::Footprint;
pub use plan::{Plan, Slot};
pub use rules::{LandingRules, ThrottleRules, RULES_PATH};
pub use thrash::Thrash;
