//! Every RPC, classified by what it needs beyond authentication.

use super::super::raw::{self, Method};

/// What an RPC needs beyond authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
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
    ("AuditService/ListAuditEntries", Admin),
    ("AuditService/ListAuditRoots", Admin),
    ("AuditService/SealAudit", Admin),
    ("AuditService/ListAuditKeys", Anyone),
    ("AuditService/WatchAudit", Admin),
    ("ProtectionService/RequestRedaction", Admin),
    ("ProtectionService/ApproveRedaction", Admin),
    ("ProtectionService/ListRedactions", Admin),
    ("ProtectionService/ClearQuarantine", Owner),
    ("ProtectionService/RotateDataKeys", Admin),
    ("SecretService/PutSecret", Admin),
    ("SecretService/DeleteSecret", Admin),
    ("SecretService/ListSecrets", Admin),
    ("ChecksService/GetTestHistory", ReadAny),
    ("ChecksService/ListFlakyTests", ReadAny),
    ("ChecksService/ListRuns", ReadAny),
    ("ChecksService/GetRun", ReadAny),
    ("ChecksService/WatchRun", ReadAny),
    ("ChecksService/ReadLog", ReadAny),
    ("ChecksService/ReadArtifact", ReadAny),
    ("ChecksService/ListNonhermetic", ReadAny),
    ("ChecksService/ListQuarantined", ReadAny),
    ("ChecksService/Quarantine", Owner),
    ("ChecksService/ReleaseQuarantine", Owner),
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
    ("RepoService/CutRelease", Owner),
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
    ("ReviewService/RequestLandStack", Land),
    ("ReviewService/GetLandStatus", ReadRoot),
    ("ReviewService/WatchReview", ReadRoot),
];

pub(super) fn class_of(method: &Method) -> Class {
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
