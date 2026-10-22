//! Structural merge on the server: two changes adding next to each other
//! in a source file both land, and the landed version says which merger
//! resolved what. With structural merge off, the second conflicts.

use rill_core::merge::STRUCTURAL_MERGE_EXTRA;
use rill_types::{ChangeVersion, ObjectHash};

use super::*;

async fn apply_on(c: &mut Clients, base: Option<&Bytes>, path: &str, body: &str) -> Bytes {
    c.changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("app"),
            base: (base.map(|v| pb::EditBase {
                base: Some(pb::edit_base::Base::Revision(hex::encode(v))),
            }))
            .into(),
            edits: vec![set_edit(path, body.as_bytes())],
            message: "edit".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap()
        .change_id
}

/// Lands a base `src/lib.rs`, then two changes on it adding one import
/// each; returns what landing the second did and the trunk head.
async fn two_imports(
    config: ServerConfig,
) -> (Result<u64, String>, Option<ChangeVersion>, TestServer) {
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let base = apply_on(&mut c, None, "src/lib.rs", "use a::A;\n\nfn f() {}\n").await;
    land(&mut c, "app", base).await;
    let ctx = server.state.repos.resolve("app").await.unwrap();
    let head = |ctx: &Arc<rill_api::RepoContext>| {
        let ctx = ctx.clone();
        async move { ctx.repo.trunk_head().await.unwrap().unwrap().1 }
    };
    let at = Bytes::copy_from_slice(head(&ctx).await.as_bytes());
    let first = apply_on(
        &mut c,
        Some(&at),
        "src/lib.rs",
        "use a::A;\nuse b::B;\n\nfn f() {}\n",
    )
    .await;
    let second = apply_on(
        &mut c,
        Some(&at),
        "src/lib.rs",
        "use a::A;\nuse c::C;\n\nfn f() {}\n",
    )
    .await;
    land(&mut c, "app", first).await;
    let seq = try_land_with(&mut c, "app", second, &[]).await;
    let landed = ctx
        .repo
        .get_change_version(&head(&ctx).await)
        .await
        .unwrap();
    (seq, landed, server)
}

#[tokio::test(flavor = "multi_thread")]
async fn adjacent_imports_from_two_changes_both_land() {
    let (seq, landed, server) = two_imports(dev_config()).await;
    assert_eq!(seq, Ok(3), "the second change landed");
    let landed = landed.unwrap();
    let ctx = server.state.repos.resolve("app").await.unwrap();
    let root = ctx
        .derive
        .get::<rill_core::RootTree>(
            &ObjectHash::from_slice(landed.version_hash().as_bytes()).unwrap(),
        )
        .await
        .unwrap();
    let Some(rill_types::EntryKind::File { content, .. }) = ctx
        .trees
        .lookup(root, &rill_types::RepoPathBuf::new("src/lib.rs").unwrap())
        .await
        .unwrap()
    else {
        panic!("src/lib.rs is missing");
    };
    let text = rill_core::read_content(ctx.repo.objects().as_ref(), &content)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&text);
    assert!(
        text.contains("use b::B;") && text.contains("use c::C;"),
        "{text}"
    );
    let record = landed.extra.get(STRUCTURAL_MERGE_EXTRA).expect("recorded");
    assert_eq!(
        String::from_utf8_lossy(record),
        format!("weave-{}: src/lib.rs", rill_merge_syntax::WEAVE_VERSION)
    );
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_structural_merge_off_the_second_import_conflicts() {
    let mut config = dev_config();
    config.merge.structural = false;
    let (seq, _, server) = two_imports(config).await;
    let refused = seq.unwrap_err();
    assert!(refused.contains("src/lib.rs"), "{refused}");
    server.stop().await;
}
