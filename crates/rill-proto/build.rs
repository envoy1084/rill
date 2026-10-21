//! Compiles `proto/rill/v1/*.proto` without a `protoc` binary.
//!
//! `protox` parses the files into a `FileDescriptorSet`, which is both fed
//! to `tonic-prost-build` for code generation and written to `OUT_DIR` so
//! the crate can embed it for gRPC reflection.

use prost::Message;
use std::env;
use std::path::PathBuf;

const PROTOS: &[&str] = &[
    "rill/v1/types.proto",
    "rill/v1/events.proto",
    "rill/v1/repo.proto",
    "rill/v1/workspace.proto",
    "rill/v1/review.proto",
    "rill/v1/intent.proto",
    "rill/v1/lander.proto",
    "rill/v1/admin.proto",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("proto");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    for proto in PROTOS {
        println!(
            "cargo:rerun-if-changed={}",
            proto_root.join(proto).display()
        );
    }

    let fds = protox::compile(PROTOS, [&proto_root])?;

    // `compile_fds` only writes the descriptor file on the protoc path, so
    // serialize it here; `lib.rs` embeds it with `include_bytes!`.
    let descriptor_path = out_dir.join("rill_v1_descriptor.bin");
    std::fs::write(&descriptor_path, fds.encode_to_vec())?;

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        // Match rill-types: every map is ordered and every byte string is
        // cheaply cloneable.
        .btree_map(".rill.v1")
        .bytes(".rill.v1")
        .compile_fds(fds)?;

    Ok(())
}
