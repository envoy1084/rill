//! The reviewer fleet's queues: one per reviewer (and resolver) agent per
//! repository, filled by the router and drained by runners over the API.

pub mod jobs;

pub use jobs::{
    enqueue_resolve, enqueue_review, resolve_queue, review_queue, FleetJob, LEASE_WAIT_MAX,
};
