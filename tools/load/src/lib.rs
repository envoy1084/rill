//! Load generation for Rill: synthetic agents at a chosen fraction of the
//! design workload, against an embedded dev server or a remote endpoint,
//! reporting throughput and latency percentiles per operation.

pub mod ledger;
pub mod profile;
pub mod run;
pub mod server;
pub mod slo;
pub mod stats;
pub mod watch;
pub mod workload;

pub use profile::Profile;
pub use run::{run, Settings};
pub use server::EmbeddedServer;
