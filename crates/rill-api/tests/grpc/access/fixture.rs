//! A repository with a public and a secret directory, its owner, and a
//! principal granted only the public one.

use rill_types::Principal;

use super::super::*;

/// A string that only exists under `secret/`.
pub const CANARY: &str = "canary-7f3a9c";

pub struct Acl {
    pub server: TestServer,
    pub repo_id: rill_types::RepoId,
    pub owner: Clients,
    /// The version holding `pub/a.txt` and `secret/s.txt`.
    pub version: Bytes,
}

pub fn alice() -> Principal {
    Principal::human("alice")
}

pub fn bob() -> Principal {
    Principal::human("bob")
}

/// Create a user of the default org.
pub async fn add_user(
    server: &TestServer,
    name: &str,
    roles: Vec<rill_types::identity::OrgRole>,
    teams: &[&str],
) -> Principal {
    let now = rill_types::Timestamp::now();
    let user = rill_types::identity::UserRecord {
        org: "default".into(),
        principal: Principal::human(name),
        email: None,
        display_name: None,
        external: Vec::new(),
        status: rill_types::identity::AccountStatus::Active,
        roles,
        teams: teams.iter().map(|t| t.to_string()).collect(),
        scim_id: None,
        external_id: None,
        created_at: now,
        updated_at: now,
    };
    server
        .state
        .identity
        .store()
        .create_user(&user, &Principal::human("admin"))
        .await
        .unwrap();
    user.principal
}

impl Acl {
    pub async fn start() -> Self {
        let server = TestServer::start(ServerConfig::default()).await;
        let mut owner = clients(&server, Some(&server.token_for(&alice()))).await;
        owner
            .repo
            .create_repo(pb::CreateRepoRequest {
                operation_id: Bytes::new(),
                repo: "acl".into(),
                description: None,
                windows_safe: false,
            })
            .await
            .unwrap();
        let version = owner
            .repo
            .apply_edits(pb::ApplyEditsRequest {
                operation_id: Bytes::new(),
                repo: "acl".into(),
                base: None,
                edits: vec![
                    set_edit("pub/a.txt", b"public\n"),
                    set_edit("secret/s.txt", format!("{CANARY}\n").as_bytes()),
                ],
                message: "seed".into(),
                intent: None,
                provenance: None,
                change_id: None,
            })
            .await
            .unwrap()
            .into_inner()
            .hash;
        let repo_id = server
            .state
            .stores
            .get_repo_by_name("acl")
            .await
            .unwrap()
            .unwrap()
            .id;
        Acl {
            server,
            repo_id,
            owner,
            version,
        }
    }

    /// Grant `relation` on directory `path` (`""` for the repository) to
    /// `who`.
    pub async fn grant(&mut self, path: &str, relation: &str, who: &Principal) {
        self.owner
            .access
            .add_relation(pb::AddRelationRequest {
                operation_id: Bytes::new(),
                relation: Some(pb::Relation {
                    repo: "acl".into(),
                    path: path.into(),
                    relation: relation.into(),
                    subject: format!("principal:{}", who.id),
                }),
            })
            .await
            .unwrap();
    }

    pub async fn as_principal(&self, who: &Principal) -> Clients {
        clients(&self.server, Some(&self.server.token_for(who))).await
    }

    pub fn at_version(&self) -> pb::RootSpec {
        pb::RootSpec {
            root: Some(pb::root_spec::Root::VersionHash(self.version.clone())),
        }
    }
}
