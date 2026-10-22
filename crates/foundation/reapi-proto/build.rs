//! Compiles the vendored REAPI, Google API and Buildbarn worker protos
//! without a `protoc` binary.

use std::env;
use std::path::PathBuf;

const PROTOS: &[&str] = &[
    "build/bazel/remote/execution/v2/remote_execution.proto",
    "google/bytestream/bytestream.proto",
    "google/rpc/error_details.proto",
    "buildbarn/remoteworker/remoteworker.proto",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("proto");
    println!("cargo:rerun-if-changed={}", proto_root.display());
    let fds = protox::compile(PROTOS, [&proto_root])?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .bytes(".")
        .compile_fds(fds)?;
    Ok(())
}
