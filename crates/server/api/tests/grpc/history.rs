//! Walking history in pages.

use super::*;

#[tokio::test]
async fn log_pages_cover_history_without_gaps_or_repeats() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "h".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let repo = common::names::repo("h");
    let mut chain: Vec<Bytes> = Vec::new();
    for i in 0..5 {
        let base = chain.last().map(|hash| pb::EditBase {
            base: Some(pb::edit_base::Base::Revision(hex::encode(hash))),
        });
        let version = c
            .changes
            .apply_edits(pb::ApplyEditsRequest {
                parent: repo.clone(),
                base: base.into(),
                edits: vec![set_edit("f.txt", format!("{i}\n").as_bytes())],
                message: format!("v{i}"),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_owned();
        chain.push(version.info.unwrap().hash);
    }

    let mut walked = Vec::new();
    let mut page_token = String::new();
    loop {
        let page = c
            .content
            .list_history(pb::ListHistoryRequest {
                repo: repo.clone(),
                revision: hex::encode(chain.last().unwrap()),
                page_size: 2,
                page_token,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_owned();
        assert!(page.versions.len() <= 2);
        walked.extend(page.versions.into_iter().map(|e| e.hash));
        if page.next_page_token.is_empty() {
            break;
        }
        page_token = page.next_page_token;
    }
    chain.reverse();
    assert_eq!(walked, chain);
}
