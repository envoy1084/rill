//! Several servers over one store: any of them serves any request.

use super::*;
use crate::common::names;

const DENY_ALL: &str = r#"forbid(principal, action == Rill::Action::"land", resource);"#;

async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !check().await {
        assert!(tokio::time::Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn servers_sharing_a_store_serve_any_request() {
    let stores = rill_store::Stores::memory();
    let a = TestServer::start_on(dev_config(), stores.clone()).await;
    let b = TestServer::start_on(dev_config(), stores.clone()).await;
    let (mut ca, mut cb) = (clients(&a, None).await, clients(&b, None).await);
    let repo = "shared";
    ca.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: repo.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let edit = |path: &str, body: &[u8]| pb::ApplyEditsRequest {
        parent: names::repo(repo),
        edits: vec![set_edit(path, body)],
        message: path.into(),
        ..Default::default()
    };
    let first = ca
        .changes
        .apply_edits(edit("a.txt", b"needle in a\n"))
        .await
        .unwrap()
        .into_owned();
    let change_id = first.version.unwrap().change_id;

    // Written on one server, landed by whichever holds the lease, and
    // reported by the other.
    assert_eq!(land(&mut cb, repo, change_id.clone()).await, 1);

    // A search on either server finds what the search writer indexed.
    for c in [&mut ca, &mut cb] {
        let hits = c
            .content
            .grep(pb::GrepRequest {
                repo: names::repo(repo),
                revision: trunk(),
                pattern: "needle".into(),
                case_insensitive: false,
                path_globs: vec![],
                max_results: 0,
                context_lines: 0,
            })
            .await
            .unwrap()
            .into_owned()
            .hits;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.txt");
    }

    // A policy set on one server binds the other.
    let second = ca
        .changes
        .apply_edits(edit("b.txt", b"b\n"))
        .await
        .unwrap()
        .into_owned();
    let change = second.version.unwrap().change_id;
    cb.review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: names::change(repo, &change),
        })
        .await
        .unwrap();
    ca.policy
        .update_policy(pb::UpdatePolicyRequest {
            policy: pb::Policy {
                name: format!("{}/policies/submit", names::repo(repo)),
                text: DENY_ALL.into(),
                ..Default::default()
            }
            .into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let review = cb.review.clone();
    eventually("the other server enforces the new policy", || {
        let review = review.clone();
        let change = change.clone();
        async move {
            let decision = review
                .evaluate_landing(pb::EvaluateLandingRequest {
                    change: names::change(repo, &change),
                })
                .await
                .unwrap()
                .into_owned()
                .decision
                .unwrap();
            !decision.allowed
        }
    })
    .await;
    a.stop().await;
    b.stop().await;
}
