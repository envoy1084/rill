//! Background tasks: per repository, the landing loop, post-land
//! verification, the derive and index job workers, event retention, the
//! follower that keeps cached policy and relations current, and durable
//! event consumers (owner-review escalation); per server, local
//! execution workers.

mod build_graph;
pub mod consumer;
mod deriver;
mod discovery;
pub mod escalation;
mod follower;
mod hooks;
mod idempotency;
mod indexer;
pub mod intents;
mod leader;
mod local_exec;
mod regenerator;
mod repo_tasks;
mod reverter;
pub(crate) mod rights;
mod verifier;

pub use build_graph::ExecTargetGraph;
pub use discovery::discover_repos;
pub use hooks::ReviewHooksAdapter;
pub use idempotency::sweep_idempotency;
pub use indexer::SEARCH_LEASE;
pub use local_exec::spawn_local_workers;
pub use regenerator::ExecRegenerator;
pub use repo_tasks::spawn_repo_workers;
pub use verifier::ExecVerifier;
