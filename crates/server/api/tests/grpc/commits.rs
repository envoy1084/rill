//! What the server checks and stamps on every commit.

use super::*;

#[tokio::test]
async fn commits_enforce_path_rules() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "win".into(),
            repo: pb::Repo {
                windows_safe: true,
                ..Default::default()
            }
            .into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let edit = |path: &'static str| {
        let changes = c.changes.clone();
        async move {
            changes
                .apply_edits(pb::ApplyEditsRequest {
                    parent: common::names::repo("win"),
                    edits: vec![set_edit(path, b"x\n")],
                    message: path.into(),
                    ..Default::default()
                })
                .await
        }
    };
    assert_eq!(
        edit("src/CON.txt").await.unwrap_err().code,
        Code::InvalidArgument
    );
    assert_eq!(
        edit(".rill/mount.json").await.unwrap_err().code,
        Code::InvalidArgument
    );
    edit("README.md").await.unwrap();
    let first = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("win"),
            edits: vec![set_edit("README.md", b"x\n"), set_edit("readme.md", b"y\n")],
            message: "case".into(),
            ..Default::default()
        })
        .await;
    assert_eq!(first.unwrap_err().code, Code::InvalidArgument);
}

#[tokio::test]
async fn server_stamps_authorship_and_attests_commits() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "auth".into(),
            repo: pb::Repo {
                windows_safe: false,
                ..Default::default()
            }
            .into(),
            ..Default::default()
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
        .changes
        .create_version(pb::CreateVersionRequest {
            request_id: String::new(),
            parent: common::names::repo("auth"),
            version: pb::ChangeVersion::from(claimed).into(),
            signatures: Vec::new(),
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap();

    let got = c
        .changes
        .get_version(pb::GetVersionRequest {
            name: common::names::version("auth", &created.hash),
            view: pb::VersionView::Full.into(),
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap();
    let stored = rill_types::ChangeVersion::try_from(got.version.unwrap()).unwrap();
    assert_eq!(stored.author.name, "dev");
    assert!(stored.created_at.millis() > 1);
    // Without a token naming a model, the client's model is a claim.
    assert_eq!(stored.provenance.model, None);
    assert_eq!(
        stored
            .provenance
            .extra
            .get("claimed_model")
            .map(String::as_str),
        Some("model-x")
    );
    assert!(stored.provenance.sponsor.is_none() && stored.provenance.resolved_by.is_none());

    let hash = ObjectHash::from_slice(&created.hash).unwrap();
    let health = c
        .admin
        .get_health(pb::GetHealthRequest {})
        .await
        .unwrap()
        .into_owned();
    let attestation = got
        .signatures
        .into_iter()
        .map(|s| rill_types::Signature::try_from(s).unwrap())
        .find(|s| s.signer.name == "rill-server")
        .expect("the server attests every commit");
    assert_eq!(hex::encode(&attestation.public_key), health.attestation_key);
    assert!(rill_policy::verify_signature(&attestation, &hash));

    // Everything landing reads was derived before the reply.
    let stores = &server.state.stores;
    let repo = stores.repo(stores.get_repo_by_name("auth").await.unwrap().unwrap().id);
    for kind in rill_core::derive::scheduled(rill_core::Schedule::LandCritical) {
        assert!(
            repo.get_derived(&kind.key, &hash).await.unwrap().is_some(),
            "{} missing after commit",
            kind.key
        );
    }

    // The commit's event and quota charge were written with it.
    let events = repo.events_after(None, 100).await.unwrap();
    assert!(events.iter().any(|(_, e)| matches!(
        e.kind,
        rill_types::EventKind::VersionStored { version, .. } if version == hash
    )));
    let dev = rill_types::Principal::human("dev");
    let today = rill_store::repo::day_of(rill_types::Timestamp::now());
    assert!(
        stores
            .usage(&dev, rill_store::repo::metric::VERSIONS, today)
            .await
            .unwrap()
            >= 1
    );
}
