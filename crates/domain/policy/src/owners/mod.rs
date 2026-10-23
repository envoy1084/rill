//! Code owners: per-directory `OWNERS.toml` files, the owner sets covering
//! each path, and whether a change's approvals satisfy them.

mod coverage;
mod file;
mod index;

pub use coverage::{
    counts_for, coverage, escalation_targets, is_entry, named_owners, Approver, SetCoverage,
};
pub use file::{parse_sla, relative, Delegation, OwnerRule, OwnersFile, OWNERS_FILE};
pub use index::{OwnerSet, OwnersIndex, OwnersTexts};
