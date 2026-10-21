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

    /// Grant `relation` on `object` (`repo` or `dir:/x`) to `who`.
    pub async fn grant(&mut self, object: &str, relation: &str, who: &Principal) {
        let object = match object {
            "repo" => "repo:acl".to_string(),
            dir => format!("path:acl/{}", dir.trim_start_matches("dir:/")),
        };
        self.owner
            .admin
            .add_relation(pb::AddRelationRequest {
                operation_id: Bytes::new(),
                relation: Some(pb::Relation {
                    object,
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
