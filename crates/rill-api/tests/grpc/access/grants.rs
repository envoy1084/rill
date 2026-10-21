//! Read grants: a path reader fetches exactly what its listings revealed,
//! by gRPC and by the object protocol, and nothing else.

use rill_types::{ObjectHash, ObjectKind};

use super::super::*;
use super::fixture::*;

fn blob_hash(bytes: &[u8]) -> ObjectHash {
    ObjectHash::digest(ObjectKind::Blob, bytes)
}

fn blob_ref(hash: &ObjectHash, size: u64) -> pb::ContentRef {
    pb::ContentRef {
        r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
            hash: Bytes::copy_from_slice(hash.as_bytes()),
            size,
        })),
    }
}

async fn read_blob(
    c: &mut Clients,
    hash: &ObjectHash,
    size: u64,
    grant: &str,
) -> Result<Vec<u8>, Code> {
    let mut stream = c
        .repo
        .get_blob(pb::GetBlobRequest {
            repo: "acl".into(),
            content: Some(blob_ref(hash, size)),
            offset: None,
            length: None,
            grant: grant.into(),
        })
        .await
        .map_err(|s| s.code())?
        .into_inner();
    let mut out = Vec::new();
    while let Some(chunk) = stream.message().await.map_err(|s| s.code())? {
        out.extend_from_slice(&chunk.data);
    }
    Ok(out)
}

async fn http_object(acl: &Acl, token: &str, kind: &str, hash: &ObjectHash, grant: &str) -> u16 {
    reqwest::Client::new()
        .get(format!(
            "http://{}/o/acl/{kind}/{}",
            acl.server.http,
            hash.to_hex()
        ))
        .bearer_auth(token)
        .header("x-rill-grant", grant)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn a_path_reader_fetches_only_what_its_listings_granted() {
    let mut acl = Acl::start().await;
    acl.grant("pub", "reader", &bob()).await;
    let mut c = acl.as_principal(&bob()).await;

    // The root lists only the way to `pub`.
    let root = c
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "acl".into(),
            root: Some(acl.at_version()),
            path: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    let names: Vec<_> = root.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["pub"]);
    let secret_listing = c
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "acl".into(),
            root: Some(acl.at_version()),
            path: "secret".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(secret_listing.code(), Code::PermissionDenied);

    let listing = c
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "acl".into(),
            root: Some(acl.at_version()),
            path: "pub".into(),
        })
        .await
        .unwrap()
        .into_inner();
    let grant = listing.read_grant;
    assert!(!grant.is_empty());

    let public_bytes = public_bytes();
    let public = blob_hash(&public_bytes);
    let secret_bytes = secret_bytes();
    let secret = blob_hash(&secret_bytes);
    assert_eq!(
        read_blob(&mut c, &public, public_bytes.len() as u64, &grant).await,
        Ok(public_bytes.clone()),
        "what the listing showed can be read"
    );
    let token = acl.server.token_for(&bob());
    assert_eq!(
        http_object(&acl, &token, "blob", &public, &grant).await,
        200
    );

    assert_eq!(
        read_blob(&mut c, &secret, secret_bytes.len() as u64, &grant).await,
        Err(Code::PermissionDenied),
        "a grant for pub/ does not open a secret hash"
    );
    assert_eq!(
        read_blob(&mut c, &public, public_bytes.len() as u64, "").await,
        Err(Code::PermissionDenied),
        "no hash is served without a grant"
    );
    assert_eq!(
        http_object(&acl, &token, "blob", &secret, &grant).await,
        403
    );
    // Someone else's grant is no use either.
    let owner_grant = acl
        .owner
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "acl".into(),
            root: Some(acl.at_version()),
            path: "secret".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .read_grant;
    assert_eq!(
        read_blob(&mut c, &secret, secret_bytes.len() as u64, &owner_grant).await,
        Err(Code::PermissionDenied)
    );
    assert_eq!(
        http_object(&acl, &token, "blob", &secret, &owner_grant).await,
        403
    );

    // The version shows only what bob may read, and grants that.
    let version = c
        .repo
        .get_change_version(pb::GetChangeVersionRequest {
            repo: "acl".into(),
            hash: acl.version.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    let paths: Vec<_> = version
        .version
        .unwrap()
        .file_changes
        .keys()
        .cloned()
        .collect();
    assert_eq!(paths, ["pub/a.txt"]);
    acl.server.stop().await;
}

#[tokio::test]
async fn a_tree_is_granted_only_when_all_of_it_is_readable() {
    let mut acl = Acl::start().await;
    acl.grant("pub", "reader", &bob()).await;
    let mut c = acl.as_principal(&bob()).await;
    let root = c
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "acl".into(),
            root: Some(acl.at_version()),
            path: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    let tree = |hash: Bytes, grant: String| {
        let mut repo = c.repo.clone();
        async move {
            repo.get_tree(pb::GetTreeRequest {
                repo: "acl".into(),
                hash,
                grant,
            })
            .await
        }
    };
    // The root tree names `secret`, so its bytes are not granted.
    assert_eq!(
        tree(root.tree_hash.clone(), root.read_grant.clone())
            .await
            .unwrap_err()
            .code(),
        Code::PermissionDenied
    );
    let pub_entry = root.entries[0].entry.clone().unwrap();
    let Some(pb::tree_entry::Kind::Dir(dir)) = pub_entry.kind else {
        panic!("pub is a directory");
    };
    let got = tree(dir.hash.clone(), root.read_grant.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(got.tree.unwrap().entries.len(), 1);
    assert!(
        !got.read_grant.is_empty(),
        "the entries are granted in turn"
    );
    acl.server.stop().await;
}
