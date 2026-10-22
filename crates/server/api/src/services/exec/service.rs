//! The service behind every execution endpoint.

use std::sync::Arc;

use rill_runtime::state::AppState;

/// Serves REAPI and the worker protocol over the server's state.
#[derive(Clone)]
pub struct ExecSvc {
    pub(super) state: Arc<AppState>,
}

impl ExecSvc {
    pub fn new(state: Arc<AppState>) -> Self {
        ExecSvc { state }
    }
}
