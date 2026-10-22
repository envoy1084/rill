//! An in-process dev server driven through the CLI as a library.

#![allow(dead_code, unused_imports)]

mod approval;
mod dev_server;
pub mod secure;

pub use approval::approve;
pub use dev_server::Server;
