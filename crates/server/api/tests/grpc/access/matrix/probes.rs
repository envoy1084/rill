//! Requests with real inputs for RPCs that validate before they authorize.

use buffa::Message;
use rill_types::{ObjectHash, RepoPathBuf, Timestamp};

use super::super::fixture::*;
use super::callers::Callers;
use crate::common::names;
use crate::*;

/// Requests with real inputs for RPCs that validate before they
/// authorize, so the matrix reaches their permission check.
pub(super) fn targeted(name: &str, callers: &Callers) -> Option<Bytes> {
    let version = callers.acl.version.clone();
    let repo = || names::repo("acl");
    let tuple = || pb::Relation {
        path: String::new(),
        relation: "reader".into(),
        subject: "*".into(),
    };
    let body = match name.trim_start_matches("rill.v1.") {
        "AccessService/CreateRelation" => pb::CreateRelationRequest {
            parent: repo(),
            relation: tuple().into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "AccessService/DeleteRelation" => pb::DeleteRelationRequest {
            parent: repo(),
            relation: tuple().into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "PolicyService/UpdatePolicy" => pb::UpdatePolicyRequest {
            policy: (pb::Policy {
                name: format!("{}/policies/submit", repo()),
                text: "permit(principal, action, resource);".into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "PolicyService/GetGuardrails" => pb::GetGuardrailsRequest {
            org: names::org("default"),
        }
        .encode_to_vec(),
        "PolicyService/UpdateGuardrails" => pb::UpdateGuardrailsRequest {
            org: names::org("default"),
            ..Default::default()
        }
        .encode_to_vec(),
        "OrgService/CreateOrg" => pb::CreateOrgRequest {
            org_id: "neworg".into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "IdentityService/UpdateIdentityProvider" => pb::UpdateIdentityProviderRequest {
            provider: (pb::IdentityProviderSpec {
                org: "default".into(),
                name: "p".into(),
                issuer: "https://idp.example".into(),
                client_id: "c".into(),
                ..Default::default()
            })
            .into(),
            allow_missing: true,
            ..Default::default()
        }
        .encode_to_vec(),
        "IdentityService/UpdateWorkloadIssuer" => pb::UpdateWorkloadIssuerRequest {
            issuer: (pb::WorkloadIssuerSpec {
                org: "default".into(),
                name: "k8s".into(),
                issuer: "https://k8s.example".into(),
                ..Default::default()
            })
            .into(),
            allow_missing: true,
            ..Default::default()
        }
        .encode_to_vec(),
        "IdentityService/UpdateUser" => pb::UpdateUserRequest {
            user: (pb::User {
                name: format!("{}/users/alice", names::org("default")),
                ..Default::default()
            })
            .into(),
            update_mask: (rill_proto::FieldMask {
                paths: vec!["status".into()],
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "SecretService/UpdateSecret" => pb::UpdateSecretRequest {
            secret: (pb::Secret {
                name: format!("{}/secrets/probe", names::org("default")),
                ..Default::default()
            })
            .into(),
            value: Bytes::from_static(b"v"),
            allow_missing: true,
            ..Default::default()
        }
        .encode_to_vec(),
        "RepoService/UpdateRef" => pb::UpdateRefRequest {
            r#ref: (pb::Ref {
                name: format!("{}/refs/probe", repo()),
                target: version.clone(),
                ..Default::default()
            })
            .into(),
            allow_missing: true,
            ..Default::default()
        }
        .encode_to_vec(),
        "NotificationService/DeleteWebhook" => pb::DeleteWebhookRequest {
            name: format!("{}/webhooks/probe", repo()),
            ..Default::default()
        }
        .encode_to_vec(),
        "ReviewService/ApplySuggestion" => pb::ApplySuggestionRequest {
            thread: format!(
                "{}/changes/{}/review/threads/probe",
                repo(),
                rill_types::ChangeId::new()
            ),
            ..Default::default()
        }
        .encode_to_vec(),
        "AttentionService/ListSimilarChanges" => pb::ListSimilarChangesRequest {
            intent: format!("{}/intents/{}", repo(), rill_types::IntentId::new()),
        }
        .encode_to_vec(),
        "AttentionService/GetIntentDigest" => pb::GetIntentDigestRequest {
            intent: format!("{}/intents/{}", repo(), rill_types::IntentId::new()),
            ..Default::default()
        }
        .encode_to_vec(),
        "AttentionService/BatchVote" => pb::BatchVoteRequest {
            intent: format!("{}/intents/{}", repo(), rill_types::IntentId::new()),
            label: "Review".into(),
            value: 1,
            ..Default::default()
        }
        .encode_to_vec(),
        "IntentService/UpdateIntent" => pb::UpdateIntentRequest {
            intent: (pb::Intent {
                name: format!("{}/intents/{}", repo(), rill_types::IntentId::new()),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "WorkspaceService/UpdateWorkspace" => pb::UpdateWorkspaceRequest {
            workspace: (pb::Workspace {
                name: callers.targets.workspace().into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "ContentService/WriteBlob" => pb::WriteBlobRequest {
            msg: Some(pb::write_blob_request::Msg::Header(pb::WriteBlobHeader {
                repo: repo(),
                ..Default::default()
            })),
        }
        .encode_to_vec(),
        "ContentService/Grep" => pb::GrepRequest {
            repo: repo(),
            pattern: CANARY.into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "ContentService/ListSymbols" => pb::ListSymbolsRequest {
            repo: repo(),
            query: "canary".into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "ContentService/FindDefinitions" => pb::FindDefinitionsRequest {
            repo: repo(),
            name: "canary".into(),
            ..Default::default()
        }
        .encode_to_vec(),
        "ContentService/GrepShards" => pb::GrepShardsRequest {
            grep: Some(pb::GrepRequest {
                repo: repo(),
                pattern: CANARY.into(),
                ..Default::default()
            })
            .into(),
            shards: vec![0],
        }
        .encode_to_vec(),
        "ChangeService/CreateVersion" => {
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
            pb::CreateVersionRequest {
                parent: repo(),
                version: pb::ChangeVersion::from(cv).into(),
                ..Default::default()
            }
            .encode_to_vec()
        }
        _ => return None,
    };
    Some(Bytes::from(body))
}
