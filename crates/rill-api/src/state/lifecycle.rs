//! A process's way out: draining, stopping and, past the deadline,
//! aborting what is still running.

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Where the process is in shutting down, and what to tell when.
pub struct Lifecycle {
    /// Cancelled to stop: listeners stop accepting, streams end, workers
    /// stop claiming and give up their leases.
    pub stop: CancellationToken,
    /// Cancelled at the shutdown deadline: running jobs are given back.
    pub abort: CancellationToken,
    /// Background tasks, so shutdown can wait for them.
    pub tasks: TaskTracker,
    /// Cancelled when draining starts.
    pub draining: CancellationToken,
}

impl Lifecycle {
    pub fn new(stop: CancellationToken) -> Self {
        Lifecycle {
            stop,
            abort: CancellationToken::new(),
            tasks: TaskTracker::new(),
            draining: CancellationToken::new(),
        }
    }

    /// Readiness turns false so load balancers send traffic elsewhere.
    pub fn start_draining(&self) {
        self.draining.cancel();
    }

    pub fn is_draining(&self) -> bool {
        self.draining.is_cancelled()
    }
}
