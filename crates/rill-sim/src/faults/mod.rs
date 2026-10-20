//! Store wrappers that inject failures chosen by the run's seed.

mod meta;
mod objects;
mod plan;

pub use meta::FaultyMetaStore;
pub use objects::FaultyObjectStore;
pub use plan::{Fault, FaultPlan};
