//! Load generation for Rill: synthetic agents at a chosen fraction of the
//! design workload, against an embedded dev server or a remote endpoint,
//! reporting throughput and latency percentiles per operation.

pub mod contended;
pub mod healing;
pub mod ledger;
pub mod profile;
pub mod run;
pub mod search_gate;
pub mod slo;
pub mod stats;
pub mod watch;
pub mod workload;

pub use profile::Profile;
pub use run::{run, Burst, Settings};
