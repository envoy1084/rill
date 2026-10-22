//! `rill.v1.LandingService`: handing reviewed changes to the lander and
//! following their requests through the queue.

mod requests;
mod service;

pub use service::LandingSvc;
