//! Small in-process mock `RepoService` and `ContentService`, in the style of the SDK's test
//! harness: one tree per root label, inline or blob content, a canned log.
//! Every other RPC answers `Unimplemented`.

#![allow(dead_code, unused_imports)]

mod content_service;
mod fixtures;
mod harness;
mod mock_repo;
mod repo_service;
mod rpc;

pub use fixtures::{fake_hash, sample_info};
pub use harness::Harness;
pub use mock_repo::MockRepo;
