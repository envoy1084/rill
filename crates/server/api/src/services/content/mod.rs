//! `rill.v1.ContentService`: files, directories, trees and blobs at a
//! revision, diffs, blame, history and search. The trait impl only
//! dispatches; each group of RPCs lives in its own file.

mod blobs;
mod compare;
mod files;
mod grep;
mod history;
mod peers;
mod service;

pub use blobs::{check_content_exists, content_bytes, BlobStream, MAX_INLINE};
pub use service::ContentSvc;
