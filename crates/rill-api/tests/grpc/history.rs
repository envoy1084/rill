//! Walking history in pages.

use super::*;

#[tokio::test]
async fn log_pages_cover_history_without_gaps_or_repeats() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "h".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let mut chain: Vec<Bytes> = Vec::new();
    for i in 0..5 {
        let base = chain.last().map(|hash| pb::EditBase {
            base: Some(pb::edit_base::Base::Root(pb::RootSpec {
                root: Some(pb::root_spec::Root::VersionHash(hash.clone())),
            })),
        });
        let version = c
            .repo
            .apply_edits(pb::ApplyEditsRequest {
                operation_id: Bytes::new(),
                repo: "h".into(),
                base,
                edits: vec![set_edit("f.txt", format!("{i}\n").as_bytes())],
                message: format!("v{i}"),
                intent: None,
                provenance: None,
                change_id: None,
            })
            .await
            .unwrap()
            .into_inner();
        chain.push(version.hash);
    }

    let mut walked = Vec::new();
    let mut start = chain.last().cloned();
    while let Some(hash) = start {
        let page = c
            .repo
            .log(pb::LogRequest {
                repo: "h".into(),
                start: Some(pb::RootSpec {
                    root: Some(pb::root_spec::Root::VersionHash(hash)),
                }),
                limit: 2,
                path: None,
                first_parent: false,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(page.entries.len() <= 2);
        walked.extend(page.entries.into_iter().map(|e| e.hash));
        start = page.next_start;
    }
    chain.reverse();
    assert_eq!(walked, chain);
}
