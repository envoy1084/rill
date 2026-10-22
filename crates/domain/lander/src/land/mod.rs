//! The landing pipeline.

mod commit;
mod domains;
pub mod followups;
mod head_config;
mod lander;
mod prepare;
mod record;
mod sequence;
mod speculate;

pub use head_config::HeadRules;
pub use lander::Lander;
pub use lander::LANDER_LEASE;
pub use record::{landing_of, list_landings, Landing, RECENT_LANDINGS};
pub use sequence::MAX_BATCH;
