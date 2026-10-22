//! Generated messages and tonic services for the Remote Execution API v2,
//! the Google APIs it uses (ByteStream, long-running operations, status)
//! and Buildbarn's worker protocol.
//!
//! Modules mirror protobuf packages, because generated code refers across
//! packages by relative path; [`reapi`], [`bytestream`], [`longrunning`],
//! [`rpc`] and [`remoteworker`] are the short names callers use.

#![allow(clippy::all, rustdoc::all, missing_docs)]

pub mod build {
    pub mod bazel {
        pub mod remote {
            pub mod execution {
                pub mod v2 {
                    tonic::include_proto!("build.bazel.remote.execution.v2");
                }
            }
        }
        pub mod semver {
            tonic::include_proto!("build.bazel.semver");
        }
    }
}

pub mod google {
    pub mod api {
        tonic::include_proto!("google.api");
    }
    pub mod bytestream {
        tonic::include_proto!("google.bytestream");
    }
    pub mod longrunning {
        tonic::include_proto!("google.longrunning");
    }
    pub mod rpc {
        tonic::include_proto!("google.rpc");
    }
}

pub mod buildbarn {
    pub mod remoteworker {
        tonic::include_proto!("buildbarn.remoteworker");
    }
}

pub use build::bazel::remote::execution::v2 as reapi;
pub use build::bazel::semver;
pub use buildbarn::remoteworker;
pub use google::{bytestream, longrunning, rpc};
