//! The conflict resolution ladder: what the lander tries before a
//! conflicted landing goes back to its owner.

mod attributes;
mod climb;
mod drivers;
mod rules;

pub use attributes::{GitAttributes, MergeAttribute, GITATTRIBUTES_PATH};
pub use climb::{Climb, Unresolved, MERGE_DRIVERS_EXTRA};
pub use drivers::union;
pub use rules::{
    Driver, DriverKind, MergeRules, Regeneration, DEFAULT_REGENERATE_TIMEOUT, MERGE_RULES_PATH,
};
