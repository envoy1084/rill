//! Remote execution: REAPI (Execution, ActionCache, ContentAddressableStorage,
//! ByteStream, Capabilities) and Buildbarn's worker protocol, over the
//! scheduler and execution spaces in `rill-exec`.

pub mod access;
mod action_cache;
mod bytestream;
pub mod capabilities;
mod cas;
mod execution;
mod operations;
pub mod resource;
mod service;
mod workers;

pub use service::ExecSvc;
