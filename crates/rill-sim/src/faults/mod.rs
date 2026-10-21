//! Store wrappers that inject failures chosen by the run's seed.

mod meta;
mod objects;
mod partition;
mod plan;

pub use meta::FaultyMetaStore;
pub use objects::FaultyObjectStore;
pub use partition::PartitionedMetaStore;
pub use plan::{Fault, FaultPlan};
