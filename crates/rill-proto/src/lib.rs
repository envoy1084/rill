//! Protobuf definitions and generated tonic services for the `rill.v1` API.
//!
//! The wire types live in [`rill::v1`]; [`convert`] maps them to and from
//! the `rill-types` object model. [`FILE_DESCRIPTOR_SET`] carries the
//! compiled schema for gRPC reflection.

pub mod convert;
pub mod error;

pub use error::{ConvertError, Result};

/// Generated protobuf messages and tonic service traits.
pub mod rill {
    /// Version 1 of the Rill API (`package rill.v1`).
    pub mod v1 {
        #![allow(clippy::large_enum_variant, clippy::doc_lazy_continuation)]
        tonic::include_proto!("rill.v1");
    }
}

/// The serialized `FileDescriptorSet` for every `rill.v1` file, for
/// reflection and dynamic clients.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/rill_v1_descriptor.bin"));
