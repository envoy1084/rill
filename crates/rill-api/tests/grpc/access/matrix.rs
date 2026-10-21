//! The permission matrix: every RPC, classified by what it needs, called
//! as every kind of caller. A new RPC fails the coverage test until it is
//! classified here. A path-limited reader's answers are searched for
//! anything from the directory it cannot read.

use prost::Message;
use rill_policy::{Capability, Verb};
use rill_types::{ObjectHash, ObjectKind, Principal, RepoPathBuf, Timestamp};
use tonic::Code;

use super::super::*;
use super::fixture::*;
use super::raw::{self, Method, Outcome};

/// What an RPC needs beyond authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// No authentication at all (sign-in).
    Open,
    /// Any authenticated caller.
    Anyone,
    /// An instance or org administrator.
    Admin,
    /// Read access to something in the repository.
    ReadAny,
    /// Read access to the repository root (also point reads given no path).
    ReadRoot,
    /// Write access somewhere.
    WriteAny,
    /// Write access to the root.
    WriteRoot,
    Review,
    Land,
    Owner,
    /// Served only for objects a read grant names; probed separately.
    Grant,
}

use Class::*;

const MATRIX: &[(&str, Class)] = &[
    ("AdminService/WhoAmI", Anyone),
    ("AdminService/IssueToken", Anyone),
    ("AdminService/GetPolicy", ReadRoot),
    ("AdminService/SetPolicy", Owner),
    ("AdminService/Health", Anyone),
    ("AdminService/CreateOrg", Admin),
    ("AdminService/ListOrgs", Anyone),
    ("AdminService/AppendTrunk", Owner),
    ("AccessService/AddRelation", Owner),
    ("AccessService/RemoveRelation", Owner),
    ("AccessService/ListRelations", Owner),
    ("AccessService/Check", Anyone),
    ("AccessService/Expand", Owner),
    ("AccessService/LookupPaths", Anyone),
    ("IdentityService/RevokeToken", Anyone),
    ("IdentityService/RevokeSession", Anyone),
    ("IdentityService/RotateTokenKeys", Admin),
    ("IdentityService/ListTokenKeys", Admin),
    ("IdentityService/PutIdentityProvider", Admin),
    ("IdentityService/ListIdentityProviders", Admin),
    ("IdentityService/DeleteIdentityProvider", Admin),
    ("IdentityService/ListUsers", Admin),
    ("IdentityService/UpdateUser", Admin),
    ("IdentityService/RegisterAgent", Anyone),
    ("IdentityService/UpdateAgent", Anyone),
    ("IdentityService/ListAgents", Anyone),
    ("IdentityService/PutWorkloadIssuer", Admin),
    ("IdentityService/ListWorkloadIssuers", Admin),
    ("IdentityService/DeleteWorkloadIssuer", Admin),
    ("IdentityService/KillSwitch", Admin),
    ("IdentityService/CreateScimCredential", Admin),
    ("IdentityService/ListScimCredentials", Admin),
    ("IdentityService/RevokeScimCredential", Admin),
    ("SessionService/ListSignInProviders", Open),
    ("SessionService/ExchangeToken", Open),
    ("SessionService/RefreshSession", Open),
    ("LanderService/GetTrunkStatus", ReadRoot),
    ("LanderService/ListLandings", ReadRoot),
    ("LanderService/GetVerification", ReadRoot),
    ("LanderService/ReportVerification", Land),
    ("LanderService/RevertSeq", Land),
    ("LanderService/Bisect", Land),
    ("IntentService/CreateIntent", WriteRoot),
    ("IntentService/GetIntent", ReadRoot),
    ("IntentService/UpdateIntent", WriteRoot),
    ("IntentService/ListIntents", ReadRoot),
    ("IntentService/LinkChange", WriteRoot),
    ("IntentService/FindSimilarIntents", ReadRoot),
    ("RepoService/GetRepo", ReadAny),
    ("RepoService/ListRepos", Anyone),
    ("RepoService/CreateRepo", Anyone),
    ("RepoService/GetChangeVersion", ReadAny),
    ("RepoService/GetChangeInfo", ReadAny),
    ("RepoService/GetChangeVersions", ReadAny),
    ("RepoService/Log", ReadAny),
    ("RepoService/PathHistory", ReadRoot),
    ("RepoService/Blame", ReadRoot),
    ("RepoService/Grep", ReadAny),
    ("RepoService/GetTree", Grant),
    ("RepoService/ListDir", ReadAny),
    ("RepoService/ResolvePath", ReadRoot),
    ("RepoService/GetBlob", Grant),
    ("RepoService/PutBlob", WriteAny),
    ("RepoService/HasObjects", WriteAny),
    ("RepoService/Diff", ReadAny),
    ("RepoService/MergeBase", ReadAny),
    ("RepoService/IsAncestor", ReadAny),
    ("RepoService/CreateChangeVersion", WriteAny),
    ("RepoService/ApplyEdits", WriteAny),
    ("RepoService/Rebase", WriteAny),
    ("RepoService/Merge", WriteAny),
    ("RepoService/Resolve", WriteAny),
    ("RepoService/ResolveRef", ReadAny),
    ("RepoService/ListRefs", ReadAny),
    ("RepoService/MoveRef", WriteRoot),
    ("RepoService/TrunkHead", ReadAny),
    ("RepoService/TrunkRange", ReadAny),
    ("RepoService/WatchRefs", ReadAny),
    ("RepoService/WatchTrunk", ReadAny),
    ("RepoService/WatchLog", ReadAny),
    ("RepoService/WatchEvents", ReadRoot),
    ("RepoService/GetDerived", ReadRoot),
    ("RepoService/WaitDerived", ReadRoot),
    ("WorkspaceService/CreateWorkspace", WriteRoot),
    ("WorkspaceService/GetWorkspace", ReadRoot),
    ("WorkspaceService/ListWorkspaces", ReadRoot),
    ("WorkspaceService/SetSparseProfile", WriteRoot),
    ("WorkspaceService/Snapshot", WriteRoot),
    ("WorkspaceService/NewChange", WriteRoot),
    ("WorkspaceService/UpdateBase", WriteRoot),
    ("WorkspaceService/Handoff", WriteRoot),
    ("WorkspaceService/Undo", WriteRoot),
    ("WorkspaceService/ListOperations", ReadRoot),
    ("WorkspaceService/DeleteWorkspace", WriteRoot),
    ("WorkspaceService/WorkspaceDiff", ReadRoot),
    ("ReviewService/GetReview", ReadRoot),
    ("ReviewService/ListReviews", ReadRoot),
    ("ReviewService/SubmitForReview", WriteAny),
    ("ReviewService/Vote", Review),
    ("ReviewService/Comment", Review),
    ("ReviewService/ReplyThread", Review),
    ("ReviewService/ResolveThread", Review),
    ("ReviewService/SetStack", WriteAny),
    ("ReviewService/EvaluateSubmit", ReadRoot),
    ("ReviewService/Abandon", WriteAny),
    ("ReviewService/RequestLand", Land),
    ("ReviewService/GetLandStatus", ReadRoot),
    ("ReviewService/WatchReview", ReadRoot),
];

fn class_of(method: &Method) -> Class {
    let short = method.name.trim_start_matches("rill.v1.");
    MATRIX
        .iter()
        .find(|(name, _)| *name == short)
        .map(|(_, c)| *c)
        .unwrap_or_else(|| panic!("{short} is not in the permission matrix"))
}

#[test]
fn every_rpc_is_classified() {
    let methods = raw::methods();
    for method in &methods {
        class_of(method);
    }
    assert_eq!(
        methods.len(),
        MATRIX.len(),
        "the matrix names RPCs that do not exist"
    );
}

/// Who is calling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Anonymous,
    /// Authenticated, with no relation to the repository.
    Stranger,
    /// Reader of `pub/` only.
    PubReader,
    /// Writer of `pub/` only.
    PubWriter,
    /// The owner, with a token narrowed to reading `pub/`.
    Narrowed,
    Owner,
}

/// Whether the permission layer lets `role` through to an RPC of `class`.
fn passes(class: Class, role: Role) -> Option<bool> {
    use Role::*;
    let allowed = match (class, role) {
        (Open, _) | (Grant, _) => return None,
        (_, Anonymous) => false,
        (Anyone, _) => true,
        (Admin, _) => false,
        (_, Owner) => true,
        (_, Stranger) => false,
        (ReadAny, PubReader | PubWriter | Narrowed) => true,
        (WriteAny, PubWriter) => true,
        _ => false,
    };
    Some(allowed)
}

fn refused(code: Code) -> bool {
    matches!(code, Code::PermissionDenied | Code::Unauthenticated)
}

struct Callers {
    acl: Acl,
    tokens: Vec<(Role, Option<String>)>,
    workspace: Bytes,
}

/// Requests with real inputs for RPCs that validate before they
/// authorize, so the matrix reaches their permission check.
fn targeted(name: &str, callers: &Callers) -> Option<Bytes> {
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

async fn callers() -> Callers {
    let mut acl = Acl::start().await;
    // Trunk holds both directories, so requests that default to it read
    // real content.
    let change = acl
        .owner
        .repo
        .get_change_version(pb::GetChangeVersionRequest {
            repo: "acl".into(),
            hash: acl.version.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .version
        .unwrap()
        .change_id;
    land(&mut acl.owner, "acl", change).await;
    let carl = Principal::human("carl");
    acl.grant("pub", "reader", &bob()).await;
    acl.grant("pub", "writer", &carl).await;
    let narrowed = Capability::repo_wide(
        acl.repo_id,
        [Verb::Read],
        Timestamp::from_millis(Timestamp::now().millis() + 600_000),
    )
    .with_paths([RepoPathBuf::new("pub").unwrap()]);
    let s = &acl.server;
    let tokens = vec![
        (Role::Anonymous, None),
        (Role::Stranger, Some(s.token_for(&Principal::human("sam")))),
        (Role::PubReader, Some(s.token_for(&bob()))),
        (Role::PubWriter, Some(s.token_for(&carl))),
        (Role::Narrowed, Some(s.token_with(&alice(), &narrowed))),
        (Role::Owner, Some(s.token_for(&alice()))),
    ];
    let workspace = acl
        .owner
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            operation_id: Bytes::new(),
            repo: "acl".into(),
            name: "probe".into(),
            base: None,
            sparse: None,
        })
        .await
        .unwrap()
        .into_inner()
        .workspace
        .unwrap()
        .id;
    Callers {
        acl,
        tokens,
        workspace,
    }
}

/// Nothing from `secret/` in an answer to someone who cannot read it.
fn assert_no_leak(what: &str, outcome: &Outcome) {
    let secret_hash = ObjectHash::digest(ObjectKind::Blob, &secret_bytes());
    let body = &outcome.body[..];
    let contains = |needle: &[u8]| body.windows(needle.len()).any(|w| w == needle);
    assert!(!contains(CANARY.as_bytes()), "{what}: the canary leaked");
    if outcome.code == Code::Ok {
        assert!(!contains(b"secret"), "{what}: a hidden path leaked");
        assert!(
            !contains(secret_hash.as_bytes()),
            "{what}: a hidden hash leaked"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_caller_gets_exactly_what_its_class_allows() {
    let callers = callers().await;
    let channel = connect(&callers.acl.server.grpc).await;
    let mut mismatches = Vec::new();
    for method in raw::methods() {
        let class = class_of(&method);
        for (role, token) in &callers.tokens {
            let Some(expected) = passes(class, *role) else {
                continue;
            };
            let probe = targeted(&method.name, &callers);
            let precise = probe.is_some();
            let body =
                probe.unwrap_or_else(|| raw::request_body(&method, "acl", &callers.workspace));
            let outcome = raw::call(channel.clone(), &method, body, token.as_deref()).await;
            let passed = !refused(outcome.code);
            // An empty request may fail validation before the permission
            // check; that is not a pass, but such an RPC needs a targeted
            // probe.
            let validated_first = !expected
                && !precise
                && matches!(
                    outcome.code,
                    Code::InvalidArgument | Code::NotFound | Code::FailedPrecondition
                );
            if *role == Role::Anonymous {
                if outcome.code != Code::Unauthenticated {
                    mismatches.push(format!("{} as anonymous: {:?}", method.name, outcome.code));
                }
                continue;
            }
            if validated_first {
                mismatches.push(format!(
                    "{} as {role:?} answered {:?} before any permission check; add a targeted probe",
                    method.name, outcome.code
                ));
            } else if passed != expected {
                mismatches.push(format!(
                    "{} as {role:?}: expected {}, got {:?}: {}",
                    method.name,
                    if expected { "through" } else { "refused" },
                    outcome.code,
                    String::from_utf8_lossy(&outcome.body)
                        .chars()
                        .take(120)
                        .collect::<String>()
                ));
            }
            if matches!(
                role,
                Role::PubReader | Role::PubWriter | Role::Narrowed | Role::Stranger
            ) {
                assert_no_leak(&format!("{} as {role:?}", method.name), &outcome);
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    callers.acl.server.stop().await;
}

/// Requests aimed at the hidden directory, by the reader of `pub/`: the
/// answers name nothing in `secret/`, whatever route they take.
#[tokio::test(flavor = "multi_thread")]
async fn a_path_reader_learns_nothing_of_the_hidden_directory() {
    let callers = callers().await;
    let channel = connect(&callers.acl.server.grpc).await;
    let token = callers
        .tokens
        .iter()
        .find(|(r, _)| *r == Role::PubReader)
        .and_then(|(_, t)| t.clone())
        .unwrap();
    let version = callers.acl.version.clone();
    let at = callers.acl.at_version();
    let secret_hash = ObjectHash::digest(ObjectKind::Blob, &secret_bytes());
    let empty_tree = pb::RootSpec {
        root: Some(pb::root_spec::Root::TreeHash(Bytes::copy_from_slice(
            rill_types::Tree::empty_hash().as_bytes(),
        ))),
    };
    let probes: Vec<(&str, Vec<u8>)> = vec![
        (
            "RepoService/ListDir",
            pb::ListDirRequest {
                repo: "acl".into(),
                root: Some(at.clone()),
                path: String::new(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/ListDir",
            pb::ListDirRequest {
                repo: "acl".into(),
                root: Some(at.clone()),
                path: "secret".into(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Log",
            pb::LogRequest {
                repo: "acl".into(),
                limit: 100,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Log",
            pb::LogRequest {
                repo: "acl".into(),
                path: Some("secret".into()),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/GetChangeVersion",
            pb::GetChangeVersionRequest {
                repo: "acl".into(),
                hash: version.clone(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/GetChangeInfo",
            pb::GetChangeInfoRequest {
                repo: "acl".into(),
                hash: version.clone(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Diff",
            pb::DiffRequest {
                repo: "acl".into(),
                base: Some(empty_tree),
                target: Some(at.clone()),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Grep",
            pb::GrepRequest {
                repo: "acl".into(),
                root: Some(at.clone()),
                pattern: CANARY.into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/ResolvePath",
            pb::ResolvePathRequest {
                repo: "acl".into(),
                root: Some(at.clone()),
                path: "secret/s.txt".into(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/PathHistory",
            pb::PathHistoryRequest {
                repo: "acl".into(),
                path: "secret/s.txt".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Blame",
            pb::BlameRequest {
                repo: "acl".into(),
                path: "secret/s.txt".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/WatchLog",
            pb::WatchLogRequest {
                repo: "acl".into(),
                after: Some(Bytes::new()),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/TrunkRange",
            pb::TrunkRangeRequest {
                repo: "acl".into(),
                from: 1,
                to: 10,
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/GetBlob",
            pb::GetBlobRequest {
                repo: "acl".into(),
                content: Some(pb::ContentRef {
                    r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                        hash: Bytes::copy_from_slice(secret_hash.as_bytes()),
                        size: secret_bytes().len() as u64,
                    })),
                }),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/HasObjects",
            pb::HasObjectsRequest {
                repo: "acl".into(),
                objects: vec![pb::ObjectRef {
                    kind: pb::ObjectKind::Blob as i32,
                    hash: Bytes::copy_from_slice(secret_hash.as_bytes()),
                }],
            }
            .encode_to_vec(),
        ),
        (
            "AccessService/LookupPaths",
            pb::LookupPathsRequest {
                repo: "acl".into(),
                relation: "reader".into(),
                subject: None,
            }
            .encode_to_vec(),
        ),
    ];
    let methods = raw::methods();
    for (name, body) in probes {
        let method = methods
            .iter()
            .find(|m| m.name == format!("rill.v1.{name}"))
            .unwrap();
        let outcome = raw::call(channel.clone(), method, Bytes::from(body), Some(&token)).await;
        assert_no_leak(name, &outcome);
    }
    let http = reqwest::Client::new()
        .get(format!(
            "http://{}/o/acl/blob/{}",
            callers.acl.server.http,
            secret_hash.to_hex()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(http.status(), 403);
    callers.acl.server.stop().await;
}
