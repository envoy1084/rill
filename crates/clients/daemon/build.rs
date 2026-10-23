//! Compiles `proto/rill/daemon/v1/daemon.proto` without a `protoc` binary,
//! the same way `rill-proto` compiles `rill.v1`.

use connectrpc_build::CodeGenConfig;
use prost::Message;
use std::env;
use std::path::PathBuf;

const PROTOS: &[&str] = &["rill/daemon/v1/daemon.proto"];

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
    let descriptor_path = out_dir.join("rill_daemon_v1_descriptor.bin");
    std::fs::write(&descriptor_path, fds.encode_to_vec())?;

    let mut buffa = CodeGenConfig::default();
    buffa.bytes_fields = vec![(".".into(), buffa_codegen::BytesRepr::Bytes)];
    buffa.map_fields = vec![(".".into(), buffa_codegen::MapRepr::BTreeMap)];
    buffa.preserve_unknown_fields = false;
    buffa.generate_json = true;
    connectrpc_build::Config::new()
        .buffa_config(buffa)
        .descriptor_set(&descriptor_path)
        .files(PROTOS)
        .include_file("_connectrpc.rs")
        .emit_rerun_directives(false)
        .compile()
        .map_err(|e| e.to_string())?;
    Ok(())
}
