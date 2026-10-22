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

    /// Count an `Execute` by how it was served.
    pub(super) fn count(&self, outcome: &'static str) {
        self.state
            .metrics
            .exec_requests
            .get_or_create(&rill_runtime::telemetry::OutcomeLabels { outcome })
            .inc();
    }
}
