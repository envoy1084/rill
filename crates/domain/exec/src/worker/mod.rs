//! Workers inside the server process, for development and single-host
//! setups; production runs Buildbarn workers against the same scheduler.

mod execute;
mod local;
mod stage;

pub use local::LocalWorker;
