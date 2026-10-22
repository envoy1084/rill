//! The scheduler: queues actions per org and platform and hands them to
//! workers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rill_jobs::{Queue, RetryPolicy};
use std::sync::Arc;

use rill_store::{Plaintext, RepoStore, Sealer, Stores};

use crate::error::Result;
use crate::remote::platform::queue_name;
use crate::remote::space::{instance_records, ExecSpace};

/// How long an org list is reused before it is read again.
const ORGS_TTL: Duration = Duration::from_secs(10);

/// Timings of the worker protocol.
#[derive(Debug, Clone, Copy)]
pub struct SchedulerConfig {
    /// How long an idle worker's request waits for work before it is told
    /// to stay idle.
    pub idle_wait: Duration,
    /// How often an executing worker reports; each report renews its claim.
    pub report_every: Duration,
    /// How long a claim lasts without a report.
    pub lease: Duration,
    /// Attempts before an action whose workers keep vanishing fails.
    pub max_attempts: u32,
    /// The timeout of an action that sets none.
    pub default_timeout: Duration,
    /// The longest timeout an action may ask for.
    pub max_timeout: Duration,
    /// The share of results entering the cache that are run again to check
    /// they are hermetic.
    pub probe_rate: f64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            idle_wait: Duration::from_secs(5),
            report_every: Duration::from_secs(10),
            lease: Duration::from_secs(45),
            max_attempts: 3,
            default_timeout: Duration::from_secs(30 * 60),
            max_timeout: Duration::from_secs(2 * 60 * 60),
            probe_rate: 0.02,
        }
    }
}

/// Queues actions and serves workers. Holds no state of its own beyond a
/// cached org list, so any server can answer any request.
pub struct Scheduler {
    pub(super) stores: Stores,
    pub(super) instance: RepoStore,
    pub(super) config: SchedulerConfig,
    /// Seals secrets and the commands that carry them.
    pub(crate) sealer: Arc<dyn Sealer>,
    orgs: Mutex<Option<(Instant, Vec<String>)>>,
    turn: AtomicUsize,
}

impl Scheduler {
    pub fn new(stores: Stores, config: SchedulerConfig) -> Self {
        Scheduler {
            instance: instance_records(&stores),
            stores,
            config,
            sealer: Arc::new(Plaintext),
            orgs: Mutex::new(None),
            turn: AtomicUsize::new(0),
        }
    }

    /// Seal secrets with `sealer` (the org data keys) rather than storing
    /// them as they are.
    pub fn with_sealer(mut self, sealer: Arc<dyn Sealer>) -> Self {
        self.sealer = sealer;
        self
    }

    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    pub fn space(&self, org: &str) -> ExecSpace {
        ExecSpace::open(&self.stores, org)
    }

    pub(super) fn queue(&self, space: &ExecSpace, name: &str) -> Queue {
        Queue::new(space.records.clone(), name).with_policy(RetryPolicy {
            max_attempts: self.config.max_attempts,
            lease: self.config.lease,
            ..RetryPolicy::default()
        })
    }

    pub(super) fn platform_queue(&self, space: &ExecSpace, platform: &str) -> Queue {
        self.queue(space, &queue_name(platform))
    }

    /// Every org, starting from a different one each call, so idle workers
    /// serve orgs in turn.
    pub(super) async fn orgs_in_turn(&self) -> Result<Vec<String>> {
        let cached = self
            .orgs
            .lock()
            .as_ref()
            .filter(|(at, _)| at.elapsed() < ORGS_TTL)
            .map(|(_, orgs)| orgs.clone());
        let mut orgs = match cached {
            Some(orgs) => orgs,
            None => {
                let mut orgs: Vec<String> = self
                    .stores
                    .list_orgs()
                    .await?
                    .into_iter()
                    .map(|o| o.name)
                    .collect();
                if !orgs.iter().any(|o| o == rill_store::DEFAULT_ORG) {
                    orgs.push(rill_store::DEFAULT_ORG.to_string());
                }
                *self.orgs.lock() = Some((Instant::now(), orgs.clone()));
                orgs
            }
        };
        if !orgs.is_empty() {
            let start = self.turn.fetch_add(1, Ordering::Relaxed) % orgs.len();
            orgs.rotate_left(start);
        }
        Ok(orgs)
    }
}
