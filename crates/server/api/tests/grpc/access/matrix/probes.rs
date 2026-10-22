//! Requests with real inputs for RPCs that validate before they authorize.

use prost::Message;
use rill_types::{ObjectHash, RepoPathBuf, Timestamp};

use super::super::fixture::*;
use super::callers::Callers;
use crate::*;

/// Requests with real inputs for RPCs that validate before they
/// authorize, so the matrix reaches their permission check.
pub(super) fn targeted(name: &str, callers: &Callers) -> Option<Bytes> {
    let version = callers.acl.version.clone();
    let tuple = || pb::Relation {
        repo: "acl".into(),
        path: String::new(),
        relation: "reader".into(),
        subject: "*".into(),
    };
    let body = match name.trim_start_matches("rill.v1.") {
        "AccessService/AddRelation" => pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: Some(tuple()),
        }
        .encode_to_vec(),
        "AccessService/RemoveRelation" => pb::RemoveRelationRequest {
            operation_id: Bytes::new(),
            relation: Some(tuple()),
        }
        .encode_to_vec(),
        "AdminService/GetPolicy" => pb::GetPolicyRequest {
            name: "acl/submit".into(),
        }
        .encode_to_vec(),
        "AdminService/SetPolicy" => pb::SetPolicyRequest {
            operation_id: Bytes::new(),
            name: "acl/submit".into(),
            text: "permit(principal, action, resource);".into(),
        }
        .encode_to_vec(),
        "ProtectionService/ClearQuarantine" => pb::ClearQuarantineRequest {
            operation_id: Bytes::new(),
            repo: "acl".into(),
            version: version.clone(),
        }
        .encode_to_vec(),
        "AdminService/CreateOrg" => pb::CreateOrgRequest {
            name: "neworg".into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "IdentityService/PutIdentityProvider" => pb::PutIdentityProviderRequest {
            operation_id: Bytes::new(),
            provider: Some(pb::IdentityProviderSpec {
                org: "default".into(),
                name: "p".into(),
                issuer: "https://idp.example".into(),
                client_id: "c".into(),
                ..Default::default()
            }),
        }
        .encode_to_vec(),
        "IdentityService/PutWorkloadIssuer" => pb::PutWorkloadIssuerRequest {
            operation_id: Bytes::new(),
            issuer: Some(pb::WorkloadIssuerSpec {
                org: "default".into(),
                name: "k8s".into(),
                issuer: "https://k8s.example".into(),
                ..Default::default()
            }),
        }
        .encode_to_vec(),
        "RepoService/PutBlob" => pb::PutBlobRequest {
            msg: Some(pb::put_blob_request::Msg::Header(pb::PutBlobHeader {
                operation_id: Bytes::new(),
                repo: "acl".into(),
                size: None,
                expected_hash: None,
            })),
        }
        .encode_to_vec(),
        "RepoService/Grep" => pb::GrepRequest {
            repo: "acl".into(),
            pattern: CANARY.into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "RepoService/Resolve" => pb::ResolveRequest {
            repo: "acl".into(),
            hash: version,
            ..Default::default()
        }
        .encode_to_vec(),
        "RepoService/CreateChangeVersion" => {
            let mut cv = rill_types::ChangeVersion {
                change_id: rill_types::ChangeId::new(),
                parents: vec![ObjectHash::from_slice(&version).unwrap()],
                predecessor: None,
                subtree_changes: Default::default(),
                file_changes: Default::default(),
                author: alice(),
                provenance: Default::default(),
                intent: None,
                message: "probe".into(),
                created_at: Timestamp::now(),
                extra: Default::default(),
            };
            cv.file_changes.insert(
                RepoPathBuf::new("pub/new.txt").unwrap(),
                rill_types::FileChange::set(
                    rill_types::ContentRef::inline(b"x".to_vec()),
                    rill_types::FileKind::Regular,
                ),
            );
            pb::CreateChangeVersionRequest {
                operation_id: Bytes::new(),
                repo: "acl".into(),
                version: Some(pb::ChangeVersion::from(cv)),
                signatures: Vec::new(),
            }
            .encode_to_vec()
        }
        _ => return None,
    };
    Some(Bytes::from(body))
}
