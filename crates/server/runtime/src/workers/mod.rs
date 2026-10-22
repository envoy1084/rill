//! Background tasks that run per repository: the landing loop, post-land
//! verification, the derive and index job workers, event retention and
//! the follower that keeps cached policy and relations current.

mod deriver;
mod discovery;
mod follower;
mod hooks;
mod idempotency;
mod indexer;
mod leader;
mod repo_tasks;
mod reverter;
pub(crate) mod rights;
mod verifier;

pub use discovery::discover_repos;
pub use hooks::ReviewHooksAdapter;
pub use idempotency::sweep_idempotency;
pub use indexer::SEARCH_LEASE;
pub use repo_tasks::spawn_repo_workers;
pub use verifier::ExecVerifier;
