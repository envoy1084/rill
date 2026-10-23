//! The resources the matrix's requests name, and which one each request
//! field takes, so a request reaches its permission check.

use rill_types::{ChangeId, IntentId, OperationId};

use super::super::fixture::Acl;
use crate::common::names;

pub(super) struct Targets {
    org: String,
    repo: String,
    workspace: String,
    version: String,
    /// A change that does not exist: its name is well formed, so handlers
    /// that parse it before authorizing get as far as the check.
    change: String,
    intent: String,
    operation: String,
}

impl Targets {
    pub(super) fn new(acl: &Acl, workspace: String) -> Self {
        let repo = names::repo("acl");
        Targets {
            org: names::org("default"),
            version: names::version("acl", &acl.version),
            change: format!("{repo}/changes/{}", ChangeId::new()),
            intent: format!("{repo}/intents/{}", IntentId::new()),
            operation: format!("{repo}/operations/{}", OperationId::new()),
            repo,
            workspace,
        }
    }

    /// The workspace the owner made for the probes.
    pub(super) fn workspace(&self) -> &str {
        &self.workspace
    }

    /// What `field` of `method`'s request names, if anything.
    pub(super) fn value(&self, method: &str, field: &str) -> Option<String> {
        let (service, rpc) = method.trim_start_matches("rill.v1.").split_once('/')?;
        let repo = &self.repo;
        let org = &self.org;
        let review = format!("{}/review", self.change);
        let org_scoped = matches!(
            service,
            "AuditService"
                | "IdentityService"
                | "SecretService"
                | "SessionService"
                | "ProtectionService"
        ) || matches!(rpc, "ListRepos" | "CreateRepo");
        Some(match (field, rpc) {
            ("repo", _) => repo.clone(),
            ("parent", _) if org_scoped => org.clone(),
            ("parent", "ListVersions") => self.change.clone(),
            ("parent", "ListWorkspaceOperations") => self.workspace.clone(),
            ("parent", "CreateThread") => review,
            ("parent", _) => repo.clone(),
            // Amending a missing change fails before the write check.
            ("change", "ApplyEdits") => return None,
            ("change", _) => self.change.clone(),
            ("review", _) => review,
            ("path", "GetOwners") => "pub/probe.rs".to_string(),
            ("landing", _) => names::position("acl", 1),
            ("run", _) => format!("{repo}/runs/probe"),
            ("version", "ReleaseVersion") => self.version.clone(),
            ("name", "GetRepo") => repo.clone(),
            ("name", "GetRef" | "DeleteRef") => format!("{repo}/refs/probe"),
            ("name", "GetTrunkPosition") => names::position("acl", 1),
            ("name", "GetVersion" | "RebaseVersion" | "ResolveConflicts") => self.version.clone(),
            (
                "name",
                "GetWorkspace" | "DeleteWorkspace" | "SnapshotWorkspace" | "StartChange"
                | "SyncWorkspace" | "HandoffWorkspace" | "DiffWorkspace",
            ) => self.workspace.clone(),
            ("name", "UndoOperation") => format!("{}/operations/head", self.workspace),
            ("name", "GetLandRequest" | "CancelLandRequest") => format!("{repo}/landRequests/1"),
            ("name", "GetReview" | "WatchReview") => review,
            ("name", "ReplyThread" | "ResolveThread") => format!("{review}/threads/probe"),
            ("name", "GetIntent" | "LinkChange") => self.intent.clone(),
            ("name", "GetRun" | "WatchRun") => format!("{repo}/runs/probe"),
            ("name", "DeleteQuarantine") => format!("{repo}/quarantines/probe"),
            ("name", "GetOperation" | "WaitOperation") => self.operation.clone(),
            ("name", "GetPolicy") => format!("{repo}/policies/submit"),
            ("name", "GetOrg") => org.clone(),
            ("name", "GetAgent") => format!("{org}/agents/probe"),
            ("name", "GetIdentityProvider" | "DeleteIdentityProvider") => {
                format!("{org}/identityProviders/probe")
            }
            ("name", "DeleteWorkloadIssuer") => format!("{org}/workloadIssuers/probe"),
            ("name", "DeleteSecret") => format!("{org}/secrets/probe"),
            ("name", "DeleteScimCredential") => format!("{org}/scimCredentials/probe"),
            ("name", "ApproveRedaction") => format!("{org}/redactions/probe"),
            _ => return None,
        })
    }
}
