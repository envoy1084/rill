//! The embedded file descriptor set.

use prost::Message;

#[test]
fn descriptor_set_is_embedded_and_parses() {
    let fds = prost_types::FileDescriptorSet::decode(crate::FILE_DESCRIPTOR_SET).unwrap();
    let names: Vec<_> = fds.file.iter().map(|f| f.name().to_string()).collect();
    for expected in [
        "rill/v1/types.proto",
        "rill/v1/repo.proto",
        "rill/v1/workspace.proto",
        "rill/v1/review.proto",
        "rill/v1/intent.proto",
        "rill/v1/lander.proto",
        "rill/v1/admin.proto",
        "rill/v1/access.proto",
        "rill/v1/audit.proto",
        "rill/v1/protection.proto",
        "rill/v1/identity.proto",
        "rill/v1/secrets.proto",
        "rill/v1/checks.proto",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
    let services: Vec<_> = fds
        .file
        .iter()
        .flat_map(|f| f.service.iter().map(|s| s.name().to_string()))
        .collect();
    assert_eq!(services.len(), 13);
}
