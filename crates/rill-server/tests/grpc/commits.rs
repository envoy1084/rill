//! What the server checks and stamps on every commit.

use super::*;

#[tokio::test]
async fn commits_enforce_path_rules() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "win".into(),
            description: None,
            windows_safe: true,
        })
        .await
        .unwrap();
    let edit = |path: &'static str| {
        let mut repo = c.repo.clone();
        async move {
            repo.apply_edits(pb::ApplyEditsRequest {
                operation_id: Bytes::new(),
                repo: "win".into(),
                base: None,
                edits: vec![set_edit(path, b"x\n")],
                message: path.into(),
                intent: None,
                provenance: None,
                change_id: None,
            })
            .await
        }
    };
    assert_eq!(
        edit("src/CON.txt").await.unwrap_err().code(),
        Code::InvalidArgument
    );
    assert_eq!(
        edit(".rill/checkout.toml").await.unwrap_err().code(),
        Code::InvalidArgument
    );
    edit("README.md").await.unwrap();
    let first = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "win".into(),
            base: None,
            edits: vec![set_edit("README.md", b"x\n"), set_edit("readme.md", b"y\n")],
            message: "case".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await;
    assert_eq!(first.unwrap_err().code(), Code::InvalidArgument);
}

#[tokio::test]
async fn server_stamps_authorship_and_attests_commits() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "auth".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();

    // A client claiming to be someone else, at another time, sponsored and
    // conflict-resolved by whoever it likes.
    let mut claimed = rill_types::ChangeVersion {
        change_id: rill_types::ChangeId::new(),
        parents: Vec::new(),
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes: Default::default(),
        author: rill_types::Principal::human("alice"),
        provenance: rill_types::Provenance {
            model: Some("model-x".into()),
            sponsor: Some(rill_types::Principal::human("ceo")),
            resolved_by: Some("human".into()),
            ..Default::default()
        },
        intent: None,
        message: "spoof".into(),
        created_at: rill_types::Timestamp::from_millis(1),
        extra: Default::default(),
    };
    claimed.file_changes.insert(
        rill_types::RepoPathBuf::new("a.txt").unwrap(),
        rill_types::FileChange::set(
            rill_types::ContentRef::inline(b"a\n".to_vec()),
            rill_types::FileKind::Regular,
        ),
    );
    let created = c
        .repo
        .create_change_version(pb::CreateChangeVersionRequest {
            operation_id: Bytes::new(),
            repo: "auth".into(),
            version: Some(pb::ChangeVersion::from(claimed)),
            signatures: Vec::new(),
        })
        .await
        .unwrap()
        .into_inner();

    let got = c
        .repo
        .get_change_version(pb::GetChangeVersionRequest {
            repo: "auth".into(),
            hash: created.hash.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    let stored = rill_types::ChangeVersion::try_from(got.version.unwrap()).unwrap();
    assert_eq!(stored.author.name, "dev");
    assert!(stored.created_at.millis() > 1);
    assert_eq!(stored.provenance.model.as_deref(), Some("model-x"));
    assert!(stored.provenance.sponsor.is_none() && stored.provenance.resolved_by.is_none());

    let hash = ObjectHash::from_slice(&created.hash).unwrap();
    let health = c
        .admin
        .health(pb::HealthRequest {})
        .await
        .unwrap()
        .into_inner();
    let attestation = got
        .signatures
        .into_iter()
        .map(|s| rill_types::Signature::try_from(s).unwrap())
        .find(|s| s.signer.name == "rill-server")
        .expect("the server attests every commit");
    assert_eq!(hex::encode(&attestation.public_key), health.attestation_key);
    assert!(rill_policy::verify_signature(&attestation, &hash));
}
