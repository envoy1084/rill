//! The embedded file descriptor set.

use prost::Message;

#[test]
fn descriptor_set_declares_every_service() {
    let fds = prost_types::FileDescriptorSet::decode(crate::FILE_DESCRIPTOR_SET).unwrap();
    let mut services: Vec<_> = fds
        .file
        .iter()
        .filter(|f| f.package() == "rill.v1")
        .flat_map(|f| f.service.iter().map(|s| s.name().to_string()))
        .collect();
    services.sort();
    assert_eq!(
        services,
        [
            "AccessService",
            "AdminService",
            "AttentionService",
            "AuditService",
            "ChangeService",
            "ChecksService",
            "ContentService",
            "EventService",
            "IdentityService",
            "IntentService",
            "LandingService",
            "OperationService",
            "OrgService",
            "PolicyService",
            "ProtectionService",
            "RepoService",
            "ReviewService",
            "ReviewerService",
            "SecretService",
            "SessionService",
            "TrunkService",
            "WorkspaceService",
        ]
    );
}
