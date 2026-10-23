//! Compiles `proto/rill/v1/*.proto` without a `protoc` binary.
//!
//! `protox` parses the files into a `FileDescriptorSet`, written to
//! `OUT_DIR`; `connectrpc-build` generates the buffa messages and the
//! Connect service traits and clients from it, and the crate embeds the
//! same set for gRPC reflection.

use connectrpc_build::CodeGenConfig;
use prost::Message;
use std::env;
use std::path::PathBuf;

const PROTOS: &[&str] = &[
    "google/rpc/status.proto",
    "google/rpc/error_details.proto",
    "rill/v1/errors.proto",
    "rill/v1/types.proto",
    "rill/v1/events.proto",
    "rill/v1/repos.proto",
    "rill/v1/content.proto",
    "rill/v1/changes.proto",
    "rill/v1/workspace.proto",
    "rill/v1/landing.proto",
    "rill/v1/review.proto",
    "rill/v1/intent.proto",
    "rill/v1/operations.proto",
    "rill/v1/trunk.proto",
    "rill/v1/admin.proto",
    "rill/v1/policy.proto",
    "rill/v1/orgs.proto",
    "rill/v1/access.proto",
    "rill/v1/audit.proto",
    "rill/v1/protection.proto",
    "rill/v1/secrets.proto",
    "rill/v1/checks.proto",
    "rill/v1/identity.proto",
    "rill/v1/session.proto",
    "rill/v1/reviewers.proto",
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
    let descriptor_path = out_dir.join("rill_v1_descriptor.bin");
    std::fs::write(&descriptor_path, fds.encode_to_vec())?;

    // Match rill-types: every map is ordered, every byte string is cheaply
    // cloneable and oneof variants hold their messages inline. Unknown
    // fields are dropped: the server and its clients are released together.
    let mut buffa = CodeGenConfig::default();
    buffa.bytes_fields = vec![(".".into(), buffa_codegen::BytesRepr::Bytes)];
    buffa.map_fields = vec![(".".into(), buffa_codegen::MapRepr::BTreeMap)];
    buffa.unboxed_oneof_fields = vec![".".into()];
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
    std::fs::write(out_dir.join("rill_v1_oneofs.rs"), oneof_modules(&fds))?;
    let services: Vec<String> = fds
        .file
        .iter()
        .flat_map(|f| {
            f.service
                .iter()
                .map(move |s| format!("{:?}", format!("{}.{}", f.package(), s.name())))
        })
        .collect();
    std::fs::write(
        out_dir.join("rill_v1_services.rs"),
        format!("&[{}]", services.join(", ")),
    )?;
    Ok(())
}

/// A module per top-level message with a oneof but no nested types, so its
/// oneof enums read as `pb::event::Kind`. buffa puts them under
/// `__buffa::oneof` and re-exports them only where the message already has
/// a module for nested types.
fn oneof_modules(fds: &prost_types::FileDescriptorSet) -> String {
    let mut out = String::new();
    let rill = fds.file.iter().filter(|f| f.package().starts_with("rill."));
    for message in rill.flat_map(|f| &f.message_type) {
        let real_oneofs = message
            .field
            .iter()
            .any(|f| f.oneof_index.is_some() && !f.proto3_optional());
        let nested = message
            .nested_type
            .iter()
            .any(|n| !n.options.as_ref().is_some_and(|o| o.map_entry()))
            || !message.enum_type.is_empty();
        if !real_oneofs || nested {
            continue;
        }
        let module = snake_case(message.name());
        out.push_str(&format!(
            "/// `{}`'s oneofs.\npub mod {module} {{\n    pub use crate::generated::rill::v1::__buffa::oneof::{module}::*;\n}}\n",
            message.name()
        ));
    }
    out
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
