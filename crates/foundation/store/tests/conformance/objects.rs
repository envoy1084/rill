//! Object store behaviour every backend must share.

use std::sync::Arc;

use bytes::Bytes;
use rill_store::{Error, ObjectStore};
use rill_types::{ObjectHash, ObjectKind};

use crate::b;

pub async fn scenario(name: &str, store: &Arc<dyn ObjectStore>) {
    let small = b(b"hello");
    let large = Bytes::from((0..200_000u32).map(|i| (i % 251) as u8).collect::<Vec<_>>());
    let empty = Bytes::new();
    for (kind, bytes) in [
        (ObjectKind::Blob, small.clone()),
        (ObjectKind::Chunk, large.clone()),
        (ObjectKind::Tree, empty.clone()),
    ] {
        let hash = ObjectHash::digest(kind, &bytes);
        assert!(!store.has(kind, &hash).await.unwrap(), "{name}: fresh");
        assert_eq!(store.get(kind, &hash).await.unwrap(), None, "{name}");
        store.put(kind, &hash, bytes.clone()).await.unwrap();
        store.put(kind, &hash, bytes.clone()).await.unwrap();
        assert!(store.has(kind, &hash).await.unwrap(), "{name}");
        assert_eq!(store.get(kind, &hash).await.unwrap(), Some(bytes), "{name}");
    }

    // Kinds are separate namespaces even for identical bytes.
    let blob_hash = ObjectHash::digest(ObjectKind::Blob, &small);
    assert!(!store.has(ObjectKind::Chunk, &blob_hash).await.unwrap());
    let as_chunk = ObjectHash::digest(ObjectKind::Chunk, &small);
    assert!(!store.has(ObjectKind::Chunk, &as_chunk).await.unwrap());

    let wrong = ObjectHash::digest(ObjectKind::Blob, b"not hello");
    let err = store
        .put(ObjectKind::Blob, &wrong, small.clone())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::HashMismatch { .. }), "{name}: {err}");
    assert!(!store.has(ObjectKind::Blob, &wrong).await.unwrap());

    let present = store
        .has_many(&[
            (ObjectKind::Blob, blob_hash),
            (ObjectKind::Blob, wrong),
            (
                ObjectKind::Chunk,
                ObjectHash::digest(ObjectKind::Chunk, &large),
            ),
        ])
        .await
        .unwrap();
    assert_eq!(present, vec![true, false, true], "{name}");
    let got = store
        .get_many(&[(ObjectKind::Blob, wrong), (ObjectKind::Blob, blob_hash)])
        .await
        .unwrap();
    assert_eq!(got, vec![None, Some(small.clone())], "{name}");

    // Each org's objects are its own: a hash known in one fetches nothing
    // in another, and the same bytes stored twice are two copies.
    let acme = store.namespaced("acme");
    let globex = store.namespaced("globex");
    assert!(
        !acme.has(ObjectKind::Blob, &blob_hash).await.unwrap(),
        "{name}: default is not acme"
    );
    let secret = b(b"acme only");
    let secret_hash = ObjectHash::digest(ObjectKind::Blob, &secret);
    acme.put(ObjectKind::Blob, &secret_hash, secret.clone())
        .await
        .unwrap();
    assert_eq!(
        globex.get(ObjectKind::Blob, &secret_hash).await.unwrap(),
        None,
        "{name}"
    );
    assert!(
        !store.has(ObjectKind::Blob, &secret_hash).await.unwrap(),
        "{name}"
    );
    assert_eq!(
        store
            .namespaced("acme")
            .get(ObjectKind::Blob, &secret_hash)
            .await
            .unwrap(),
        Some(secret),
        "{name}: a namespace is the same wherever it is opened"
    );
    assert!(
        store
            .namespaced("default")
            .has(ObjectKind::Blob, &blob_hash)
            .await
            .unwrap(),
        "{name}: the base store is the default org's"
    );
}
