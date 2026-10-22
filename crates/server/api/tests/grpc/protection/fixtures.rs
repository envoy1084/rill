//! Shared fixtures for the protection scenarios.

use rill_types::{ObjectHash, ObjectKind};

use super::super::*;

pub const AWS_KEY: &str = "AKIAZ7T3KQ2MVXC4L5PW";

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
) -> Result<pb::ApplyEditsResponse, ConnectError> {
    c.repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: None.into(),
            edits,
            message: "edit".into(),
            intent: None,
            provenance: None.into(),
            change_id: None,
        })
        .await
        .map(|r| r.into_owned())
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
            content: (pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                    hash: Bytes::copy_from_slice(hash.as_bytes()),
                    size: bytes.len() as u64,
                })),
            })
            .into(),
            kind: pb::FileKind::Regular.into(),
            copy_from: None.into(),
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
) -> Result<pb::ApplyEditsResponse, ConnectError> {
    c.repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: (base.map(|v| pb::EditBase {
                base: Some(pb::edit_base::Base::Root(pb::RootSpec {
                    root: Some(pb::root_spec::Root::VersionHash(v.clone())),
                })),
            }))
            .into(),
            edits,
            message: message.into(),
            intent: None,
            provenance: None.into(),
            change_id: None,
        })
        .await
        .map(|r| r.into_owned())
}

/// `n` distinct, well-formed fake AWS access key ids (base32 after the
/// prefix, as the detector requires).
pub fn fake_aws_keys(n: u64) -> Vec<String> {
    const BASE32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    (0..n)
        .map(|i| {
            let mut x = 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(i + 1);
            let tail: String = (0..16)
                .map(|_| {
                    x = x.rotate_left(5) ^ 0xD6E8_FEB8_6659_FD93;
                    BASE32[(x % 32) as usize] as char
                })
                .collect();
            format!("AKIA{tail}")
        })
        .collect()
}
