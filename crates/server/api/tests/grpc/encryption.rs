//! Objects at rest: sealed under each org's own data keys, readable by any
//! server holding the key service, and by no other org.

use std::path::Path;

use rill_api::StorageConfig;
use rill_identity::keys::{DataKeyManager, KeyServiceConfig, LocalKeyService};
use rill_runtime::config::IdentityConfig;
use rill_store::{LocalStores, ObjectStore, ObjectStoreBackend, OrgSealer};
use rill_types::Principal;

use super::*;

const MASTER: [u8; 32] = [0x17; 32];

fn sealed_config(dir: &Path) -> ServerConfig {
    ServerConfig {
        storage: StorageConfig::Local {
            dir: dir.to_path_buf(),
        },
        identity: IdentityConfig {
            keys: KeyServiceConfig::Local {
                master_key_hex: Some(hex::encode(MASTER)),
                master_key_file: None,
            },
            admins: vec!["root".into()],
            ..IdentityConfig::default()
        },
        ..ServerConfig::default()
    }
}

/// The objects directory as another server would open it, with `master`.
async fn reopened(dir: &Path, stores: &rill_store::Stores, master: [u8; 32]) -> ObjectStoreBackend {
    let sealer = Arc::new(OrgSealer::new());
    sealer.set_source(Arc::new(DataKeyManager::new(
        stores.identity(),
        Arc::new(LocalKeyService::new(&master).unwrap()),
        Principal::service("test"),
    )));
    ObjectStoreBackend::local(dir.join(LocalStores::OBJECTS_DIR))
        .await
        .unwrap()
        .with_sealer(sealer)
}

fn files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn objects_are_sealed_per_org_and_open_only_where_they_were_written() {
    let dir = tempfile::tempdir().unwrap();
    let server = TestServer::start(sealed_config(dir.path())).await;
    let stores = server.state.stores.clone();
    stores.create_org("acme", "").await.unwrap();

    let marker = "PLAINTEXT-MARKER-0123456789 ".repeat(40);
    let c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            request_id: String::new(),
            parent: "orgs/default".into(),
            repo_id: "app".into(),
            repo: None.into(),
        })
        .await
        .unwrap();
    c.changes
        .apply_edits(pb::ApplyEditsRequest {
            request_id: String::new(),
            parent: common::names::repo("app"),
            base: None.into(),
            edits: vec![set_edit("notes.txt", marker.as_bytes())],
            message: "notes".into(),
            intent: String::new(),
            provenance: None.into(),
            change: String::new(),
        })
        .await
        .unwrap();
    let acme_bytes = Bytes::from("ACME-ONLY-MARKER ".repeat(40));
    let acme_hash = ObjectHash::digest(ObjectKind::Blob, &acme_bytes);
    let acme = stores.org("acme");
    acme.objects
        .put(ObjectKind::Blob, &acme_hash, acme_bytes.clone())
        .await
        .unwrap();

    // Nothing on disk is readable without the keys, and each org has its own.
    let objects = dir.path().join(LocalStores::OBJECTS_DIR);
    for file in files(&objects) {
        let raw = std::fs::read(&file).unwrap();
        assert!(&raw[..4] == b"RXE1", "{} is not sealed", file.display());
        for needle in [&b"PLAINTEXT-MARKER"[..], b"ACME-ONLY", b"notes"] {
            assert!(
                !raw.windows(needle.len()).any(|w| w == needle),
                "{}",
                file.display()
            );
        }
    }
    let identity = stores.identity();
    assert_eq!(identity.data_keys("default").await.unwrap().len(), 1);
    assert_eq!(identity.data_keys("acme").await.unwrap().len(), 1);

    // A sealed object copied into another org's namespace does not open.
    let hash = ObjectHash::digest(ObjectKind::Blob, marker.as_bytes());
    let rel = |ns: &str| {
        let hex = hash.to_hex();
        objects.join(ns).join("blob").join(&hex[..2]).join(hex)
    };
    std::fs::create_dir_all(rel("acme").parent().unwrap()).unwrap();
    std::fs::copy(rel("default"), rel("acme")).unwrap();
    assert!(acme.objects.get(ObjectKind::Blob, &hash).await.is_err());

    // A rotated key waits out its activation; old objects still open, and
    // every key can be wrapped again.
    let rotated = c
        .protection
        .rotate_data_keys(pb::RotateDataKeysRequest {
            request_id: String::new(),
            parent: "orgs/default".into(),
            rewrap: false,
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(rotated.key_id, Some(2));
    let rewrapped = c
        .protection
        .rotate_data_keys(pb::RotateDataKeysRequest {
            request_id: String::new(),
            parent: String::new(),
            rewrap: true,
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(rewrapped.rewrapped, 3);

    // Another server with the key service opens everything; one without
    // it opens nothing.
    let other = reopened(dir.path(), &stores, MASTER).await;
    assert_eq!(
        other.get(ObjectKind::Blob, &hash).await.unwrap().unwrap(),
        marker.as_bytes()
    );
    assert_eq!(
        other
            .namespaced("acme")
            .get(ObjectKind::Blob, &acme_hash)
            .await
            .unwrap()
            .unwrap(),
        acme_bytes
    );
    let stranger = reopened(dir.path(), &stores, [0x18; 32]).await;
    assert!(stranger.get(ObjectKind::Blob, &hash).await.is_err());
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dev_mode_stores_plaintext_unless_asked() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = sealed_config(dir.path());
    config.dev_mode = true;
    let server = TestServer::start(config).await;
    assert!(server.state.data_keys.is_none());
    server.stop().await;

    let dir = tempfile::tempdir().unwrap();
    let mut config = sealed_config(dir.path());
    config.dev_mode = true;
    config.encryption.enabled = Some(true);
    let server = TestServer::start(config).await;
    assert!(server.state.data_keys.is_some());
    server.stop().await;
}
