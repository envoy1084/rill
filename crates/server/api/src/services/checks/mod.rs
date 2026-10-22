//! `rill.v1.ChecksService`: what verification ran, for anyone who can
//! read the repository: test history and flake scores, and runs with
//! their progress, logs and artifacts.

mod convert;
mod history;
mod quarantine;
mod runs;
mod service;

pub use service::ChecksSvc;
