//! Background tasks: per repository, the landing loop, post-land
//! verification, the derive and index job workers, event retention, the
//! follower that keeps cached policy and relations current, and durable
//! event consumers (owner-review escalation, intent budgets, reviewer
//! routing and calibration, landing decisions, audit sampling,
//! notifications) and the notification delivery and digest tasks; per server, local
//! execution workers.

pub mod audits;
mod build_graph;
pub mod calibration;
pub mod consumer;
pub mod decisions;
mod deriver;
pub mod digests;
mod discovery;
pub mod escalation;
mod follower;
mod hooks;
mod idempotency;
mod indexer;
pub mod intents;
mod leader;
mod local_exec;
pub mod notifier;
mod regenerator;
mod repo_tasks;
mod reverter;
pub mod reviewers;
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
