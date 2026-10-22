//! A small in-process mock `RepoService`, in the style of the SDK's test
//! harness: one tree per root label, inline or blob content, a canned log.
//! Every other RPC answers `Unimplemented`.

#![allow(dead_code, unused_imports)]

mod fixtures;
mod harness;
mod mock_repo;
mod repo_service;
mod rpc;

pub use fixtures::{fake_hash, sample_info};
pub use harness::Harness;
pub use mock_repo::MockRepo;
