//! Protobuf definitions and generated Connect services for the `rill.v1`
//! API.
//!
//! The wire types, service traits and clients live in [`rill::v1`];
//! [`convert`] maps the messages to and from
//! the `rill-types` object model. [`FILE_DESCRIPTOR_SET`] carries the
//! compiled schema for gRPC reflection.

pub mod convert;
pub mod error;

pub use error::{ConvertError, Result};

#[allow(
    clippy::large_enum_variant,
    clippy::doc_lazy_continuation,
    clippy::all,
    missing_docs
)]
mod generated {
    connectrpc::include_generated!();
}

/// Generated protobuf messages, Connect service traits and clients.
pub mod rill {
    /// Version 1 of the Rill API (`package rill.v1`), with each message's
    /// oneofs in a module named after it (`event::Kind`).
    pub mod v1 {
        pub use crate::generated::rill::v1::*;
        include!(concat!(env!("OUT_DIR"), "/rill_v1_oneofs.rs"));
    }
}

/// Every `rill.v1` service, fully qualified (`rill.v1.RepoService`).
pub const SERVICES: &[&str] = include!(concat!(env!("OUT_DIR"), "/rill_v1_services.rs"));

/// The serialized `FileDescriptorSet` for every `rill.v1` file, for
/// reflection and dynamic clients.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/rill_v1_descriptor.bin"));
