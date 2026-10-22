//! Remote execution workers: they present a configured secret instead of a
//! token, act for every org, and alone may write the action cache.

use rill_policy::AuthContext;
use rill_types::Principal;

/// Marks a request authenticated as a remote execution worker.
#[derive(Debug, Clone, Copy)]
pub struct ExecWorker;

/// The principal workers act as.
pub const WORKER_PRINCIPAL: &str = "exec-worker";

/// The caller context of a worker.
pub fn worker_context() -> AuthContext {
    AuthContext {
        principal: Principal::service(WORKER_PRINCIPAL),
        teams: Vec::new(),
        capability: None,
        claims: None,
        scope: None,
        relations_seen: Vec::new(),
    }
}

/// Whether `request` came from a worker.
pub fn is_worker<T>(request: &tonic::Request<T>) -> bool {
    request.extensions().get::<ExecWorker>().is_some()
}
