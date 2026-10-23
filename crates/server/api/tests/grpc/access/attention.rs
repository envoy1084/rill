//! Human attention: a landing approved only by agents is sampled for an
//! audit that shows in the owners' inboxes; failing it counts against the
//! approving agents and shows in the intent's digest; look-alike changes
//! are approved in one batch.

use rill_types::Principal;

use super::super::*;
use super::fixture::*;
use super::intents::{approve, change_under, create_intent};
use super::reviewers::add_agent;
use super::writes::vote;

async fn inbox(c: &mut Clients) -> Vec<pb::InboxItem> {
    c.attention
        .list_inbox(pb::ListInboxRequest {
            parent: common::names::repo("acl"),
            limit: 0,
        })
        .await
        .unwrap()
        .into_owned()
        .items
}

#[tokio::test]
async fn agent_only_landings_are_audited_and_look_alikes_batched() {
    let mut acl = Acl::start().await;
    add_user(&acl.server, "ann", vec![], &[]).await;
    add_user(&acl.server, "cara", vec![], &[]).await;
    add_agent(&acl.server, "coder", "cara", None).await;
    add_agent(&acl.server, "sec", "ann", Some("claude")).await;
    add_agent(&acl.server, "perf", "ann", Some("gpt")).await;
    acl.grant("pub", "writer", &Principal::agent("coder")).await;
    for a in ["sec", "perf"] {
        acl.grant("", "reviewer", &Principal::agent(a)).await;
    }
    let intent = create_intent(
        &mut acl.owner,
        pb::IntentScope {
            paths: vec!["pub/**".into()],
            human_sample_permille: Some(1000),
            ..Default::default()
        },
    )
    .await;
    approve(&mut acl.owner, &intent.name, 1).await.unwrap();

    let mut coder = acl.as_principal(&Principal::agent("coder")).await;
    let change = change_under(&mut coder, &intent.name, &["pub/new.rs"]).await;
    for a in ["sec", "perf"] {
        let mut c = acl.as_principal(&Principal::agent(a)).await;
        vote(&mut c, &change, "Review", 1).await.unwrap();
    }
    let mut sec = acl.as_principal(&Principal::agent("sec")).await;
    vote(&mut sec, &change, "Verified", 1).await.unwrap();
    let seq = common::request_land(&mut acl.owner, "acl", change.clone())
        .await
        .unwrap();
    let landing = common::names::position("acl", seq);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let item = loop {
        let items = inbox(&mut acl.owner).await;
        if let Some(i) = items.iter().find(|i| i.landing == landing) {
            break i.clone();
        }
        assert!(std::time::Instant::now() < deadline, "{items:?}");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    assert_eq!(item.kind, buffa::EnumValue::from(pb::InboxKind::Audit));
    acl.grant("", "reader", &bob()).await;
    let mut bob = acl.as_principal(&bob()).await;
    assert!(inbox(&mut bob).await.is_empty(), "not bob's to audit");
    let refused = acl
        .as_principal(&Principal::agent("perf"))
        .await
        .attention
        .record_audit(pb::RecordAuditRequest {
            landing: landing.clone(),
            passed: true,
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(
        refused.code,
        Code::PermissionDenied,
        "an approver cannot audit"
    );
    let audit = acl
        .owner
        .attention
        .record_audit(pb::RecordAuditRequest {
            landing: landing.clone(),
            passed: false,
            note: "misses a bounds check".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .audit
        .unwrap();
    assert_eq!(audit.status, "failed");
    let stats = acl
        .owner
        .reviewers
        .list_reviewer_stats(pb::ListReviewerStatsRequest {
            parent: common::names::repo("acl"),
        })
        .await
        .unwrap()
        .into_owned()
        .reviewers;
    assert!(
        stats.iter().all(|s| s.escapes == 1),
        "a failed audit is an escape for each approver: {stats:?}"
    );
    let digest = acl
        .owner
        .attention
        .get_intent_digest(pb::GetIntentDigestRequest {
            intent: intent.name.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(
        (digest.landed, digest.reverted, digest.sampled),
        (1, 1, 1),
        "{digest:?}"
    );

    // Two changes making the same edit in different files are one group.
    let a = change_under(&mut coder, &intent.name, &["pub/a.rs"]).await;
    let b = change_under(&mut coder, &intent.name, &["pub/b.rs"]).await;
    // Changes join the intent through the event log, so wait for both.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let groups = loop {
        let groups = acl
            .owner
            .attention
            .list_similar_changes(pb::ListSimilarChangesRequest {
                intent: intent.name.clone(),
            })
            .await
            .unwrap()
            .into_owned()
            .groups;
        if groups.first().is_some_and(|g| g.changes.len() == 2) {
            break groups;
        }
        assert!(std::time::Instant::now() < deadline, "{groups:?}");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    let results = acl
        .owner
        .attention
        .batch_vote(pb::BatchVoteRequest {
            intent: intent.name.clone(),
            signature: groups[0].signature.clone(),
            label: "Review".into(),
            value: 2,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .results;
    assert!(results.iter().all(|r| r.voted), "{results:?}");
    let named: Vec<String> = [a, b]
        .iter()
        .map(|c| common::names::change("acl", c))
        .collect();
    assert!(results.iter().all(|r| named.contains(&r.change)));
    acl.server.stop().await;
}
