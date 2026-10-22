//! In-process mock services for SDK tests.
//!
//! `MockRepo` serves trees keyed by root label, a blob store, a canned log
//! and echoing writes; `MockReview` serves land tickets with scripted
//! status sequences. Every other RPC answers `Unimplemented`.

#![allow(dead_code, unused_imports)]

mod change_service;
mod content_service;
mod fixtures;
mod harness;
mod mock_repo;
mod mock_review;
mod repo_service;
mod rpc;

pub use fixtures::{fake_hash, sample_info};
pub use harness::Harness;
pub use mock_repo::{MockFile, MockRepo};
pub use mock_review::MockReview;
