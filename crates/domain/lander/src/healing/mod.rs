//! Self-healing trunk: incidents for culprits post-land verification
//! finds, trunk's health because of them, and what a landing fixes.

mod fixes;
mod incidents;
mod state;

pub use fixes::{fixes_of, is_revert, FIXES_KEY};
pub use state::{judge, GuardRules};
