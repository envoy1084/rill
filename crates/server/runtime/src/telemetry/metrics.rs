//! The server's metric families and their Prometheus text encoding.

use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;

/// One RPC method and how it ended.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RpcLabels {
    pub service: String,
    pub method: String,
    pub code: String,
}

/// One RPC method.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MethodLabels {
    pub service: String,
    pub method: String,
}

/// How a metadata transaction ended.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct OutcomeLabels {
    pub outcome: &'static str,
}

/// One repository.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RepoLabels {
    pub repo: String,
}

/// One queue of one repository.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct QueueLabels {
    pub repo: String,
    pub queue: String,
}

/// One conflict domain, or one sponsor's share, of one repository.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct KeyLabels {
    pub repo: String,
    pub key: String,
}

/// One file extension and what structural merge did with it.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct OutcomeKeyLabels {
    pub extension: String,
    pub outcome: String,
}

/// One lease role of one repository.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RoleLabels {
    pub repo: String,
    pub role: String,
}

type Timings<L> = Family<L, Histogram, fn() -> Histogram>;

fn latency() -> Histogram {
    Histogram::new(exponential_buckets(0.0005, 2.0, 18))
}

/// Every metric the server keeps.
pub struct Metrics {
    registry: Registry,
    names: Vec<String>,
    pub rpc_requests: Family<RpcLabels, Counter>,
    pub rpc_duration: Timings<MethodLabels>,
    pub meta_transactions: Family<OutcomeLabels, Counter>,
    pub meta_transaction_duration: Histogram,
    pub jobs_pending: Family<QueueLabels, Gauge>,
    pub jobs_dead: Family<QueueLabels, Gauge>,
    pub jobs_retried: Family<QueueLabels, Gauge>,
    pub land_requests_pending: Family<RepoLabels, Gauge>,
    pub trunk_head: Family<RepoLabels, Gauge>,
    pub events_written: Family<RepoLabels, Gauge>,
    pub lease_epoch: Family<RoleLabels, Gauge>,
    /// Queue-to-trunk time of recent landings, p50 and p99, and the mean
    /// time between landings (milliseconds; from the lease holder).
    pub land_wait_p50_ms: Family<RepoLabels, Gauge>,
    pub land_wait_p99_ms: Family<RepoLabels, Gauge>,
    pub land_interval_ms: Family<RepoLabels, Gauge>,
    /// Share of recent rebases that conflicted, per domain (per mille).
    pub land_conflict_permille: Family<KeyLabels, Gauge>,
    /// Unfinished land requests per sponsor (or requester).
    pub land_queued_by_share: Family<KeyLabels, Gauge>,
    /// Text conflicts structural merge resolved, declined or skipped, by
    /// file extension and outcome.
    pub structural_merges: Family<OutcomeKeyLabels, Gauge>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        let mut registry = Registry::with_prefix("rill");
        let mut names = Vec::new();
        let rpc_requests = Family::<RpcLabels, Counter>::default();
        register(
            &mut registry,
            &mut names,
            "rpc_requests",
            "gRPC requests by method and status code",
            rpc_requests.clone(),
        );
        let rpc_duration: Timings<MethodLabels> = Family::new_with_constructor(latency);
        register(
            &mut registry,
            &mut names,
            "rpc_duration_seconds",
            "gRPC request latency",
            rpc_duration.clone(),
        );
        let meta_transactions = Family::<OutcomeLabels, Counter>::default();
        register(
            &mut registry,
            &mut names,
            "meta_transactions",
            "metadata transactions by outcome: committed, conflict or error",
            meta_transactions.clone(),
        );
        let meta_transaction_duration = latency();
        register(
            &mut registry,
            &mut names,
            "meta_transaction_duration_seconds",
            "metadata transaction latency",
            meta_transaction_duration.clone(),
        );
        let queue_gauge = |registry: &mut Registry, names: &mut Vec<String>, name, help| {
            let family = Family::<QueueLabels, Gauge>::default();
            register(registry, names, name, help, family.clone());
            family
        };
        let jobs_pending = queue_gauge(
            &mut registry,
            &mut names,
            "jobs_pending",
            "jobs enqueued and not finished",
        );
        let jobs_dead = queue_gauge(
            &mut registry,
            &mut names,
            "jobs_dead",
            "jobs in the dead-letter queue",
        );
        let jobs_retried = queue_gauge(
            &mut registry,
            &mut names,
            "jobs_retried",
            "job attempts retried so far",
        );
        let repo_gauge = |registry: &mut Registry, names: &mut Vec<String>, name, help| {
            let family = Family::<RepoLabels, Gauge>::default();
            register(registry, names, name, help, family.clone());
            family
        };
        let land_requests_pending = repo_gauge(
            &mut registry,
            &mut names,
            "land_requests_pending",
            "land requests not finished",
        );
        let trunk_head = repo_gauge(
            &mut registry,
            &mut names,
            "trunk_head",
            "sequence number of the trunk head",
        );
        let events_written = repo_gauge(
            &mut registry,
            &mut names,
            "events_written",
            "events written to the log so far",
        );
        let land_wait_p50_ms = repo_gauge(
            &mut registry,
            &mut names,
            "land_wait_p50_ms",
            "median queue-to-trunk time of recent landings",
        );
        let land_wait_p99_ms = repo_gauge(
            &mut registry,
            &mut names,
            "land_wait_p99_ms",
            "p99 queue-to-trunk time of recent landings",
        );
        let land_interval_ms = repo_gauge(
            &mut registry,
            &mut names,
            "land_interval_ms",
            "mean time between recent landings",
        );
        let key_gauge = |registry: &mut Registry, names: &mut Vec<String>, name, help| {
            let family = Family::<KeyLabels, Gauge>::default();
            register(registry, names, name, help, family.clone());
            family
        };
        let land_conflict_permille = key_gauge(
            &mut registry,
            &mut names,
            "land_conflict_permille",
            "share of recent rebases in a conflict domain that conflicted",
        );
        let land_queued_by_share = key_gauge(
            &mut registry,
            &mut names,
            "land_queued_by_share",
            "unfinished land requests per sponsor or requester",
        );
        let structural_merges = Family::<OutcomeKeyLabels, Gauge>::default();
        register(
            &mut registry,
            &mut names,
            "structural_merges",
            "text conflicts structural merge resolved, declined or skipped",
            structural_merges.clone(),
        );
        let lease_epoch = Family::<RoleLabels, Gauge>::default();
        register(
            &mut registry,
            &mut names,
            "lease_epoch",
            "epoch of each lease role; a change is a takeover",
            lease_epoch.clone(),
        );
        Metrics {
            registry,
            names,
            rpc_requests,
            rpc_duration,
            meta_transactions,
            meta_transaction_duration,
            jobs_pending,
            jobs_dead,
            jobs_retried,
            land_requests_pending,
            trunk_head,
            events_written,
            lease_epoch,
            land_wait_p50_ms,
            land_wait_p99_ms,
            land_interval_ms,
            land_conflict_permille,
            land_queued_by_share,
            structural_merges,
        }
    }

    /// The name of every metric family, as exported.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The Prometheus text exposition of every metric.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        // Writing into a String cannot fail.
        let _ = prometheus_client::encoding::text::encode(&mut out, &self.registry);
        out
    }
}

fn register(
    registry: &mut Registry,
    names: &mut Vec<String>,
    name: &str,
    help: &str,
    metric: impl prometheus_client::registry::Metric,
) {
    names.push(format!("rill_{name}"));
    registry.register(name, help, metric);
}
