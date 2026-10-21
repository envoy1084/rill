//! Shared fixtures for the protection scenarios.

use rill_types::{ObjectHash, ObjectKind};

use super::super::*;

pub const AWS_KEY: &str = "AKIAZ7T3KQ2MVXC4L9PW";

pub fn admins_config() -> ServerConfig {
    let mut config = ServerConfig::default();
    config.identity.admins = vec!["root".into(), "sec".into()];
    config
}

pub async fn repo(c: &mut Clients, name: &str) {
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: name.into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
}

pub async fn apply(
    c: &mut Clients,
    repo: &str,
    edits: Vec<pb::Edit>,
) -> Result<pb::ApplyEditsResponse, Status> {
    c.repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: None,
            edits,
            message: "edit".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .map(|r| r.into_inner())
}

/// An edit storing `bytes` as a blob uploaded beforehand.
pub async fn blob(server: &TestServer, path: &str, bytes: &[u8]) -> (pb::Edit, ObjectHash) {
    let hash = ObjectHash::digest(ObjectKind::Blob, bytes);
    server
        .state
        .stores
        .objects
        .put(ObjectKind::Blob, &hash, Bytes::copy_from_slice(bytes))
        .await
        .unwrap();
    let edit = pb::Edit {
        path: path.into(),
        op: Some(pb::edit::Op::SetContent(pb::SetContent {
            content: Some(pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                    hash: Bytes::copy_from_slice(hash.as_bytes()),
                    size: bytes.len() as u64,
                })),
            }),
            kind: pb::FileKind::Regular as i32,
            copy_from: None,
        })),
    };
    (edit, hash)
}

/// Land `.rill/secrets.toml` with `text` on `repo`'s trunk.
pub async fn land_rules(c: &mut Clients, repo: &str, text: &str) {
    let rules = apply(
        c,
        repo,
        vec![set_edit(".rill/secrets.toml", text.as_bytes())],
    )
    .await
    .unwrap();
    land_with(c, repo, rules.version.unwrap().change_id, &[("Owner", 1)]).await;
}

/// [`apply`] on top of `base`, with `message`.
pub async fn apply_on(
    c: &mut Clients,
    repo: &str,
    base: Option<&Bytes>,
    edits: Vec<pb::Edit>,
    message: &str,
) -> Result<pb::ApplyEditsResponse, Status> {
    c.repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: base.map(|v| pb::EditBase {
                base: Some(pb::edit_base::Base::Root(pb::RootSpec {
                    root: Some(pb::root_spec::Root::VersionHash(v.clone())),
                })),
            }),
            edits,
            message: message.into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .map(|r| r.into_inner())
}
