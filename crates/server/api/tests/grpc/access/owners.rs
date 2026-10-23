//! Code owners: required approvals per owner set, and escalation of
//! owner reviews past their SLA.

use super::super::*;
use super::fixture::*;
use super::writes::{change_of, edit, vote};
#[tokio::test]
async fn an_owners_rule_holds_landing_until_its_required_approvals() {
    let mut acl = Acl::start().await;
    for (who, teams) in [
        ("ann", &["billing"][..]),
        ("dan", &["billing"]),
        ("rev", &[]),
    ] {
        add_user(&acl.server, who, vec![], teams).await;
        acl.grant("", "reviewer", &rill_types::Principal::human(who))
            .await;
    }
    acl.grant("pub", "writer", &bob()).await;
    assert_eq!(
        edit(
            &mut acl.owner,
            None,
            vec![set_edit("pub/OWNERS.toml", b"owners = [\"ann\"]\n")]
        )
        .await,
        Err(Code::InvalidArgument),
        "a malformed owners file is refused when stored"
    );
    let owners = edit(
        &mut acl.owner,
        None,
        vec![set_edit(
            "pub/OWNERS.toml",
            b"owners = [\"team:billing\"]\n[[rule]]\npaths = [\"**\"]\nowners = [\"team:billing\"]\nrequired = 2\n",
        )],
    )
    .await
    .unwrap();
    let owners_change = change_of(&mut acl.owner, &owners).await;
    land_with(&mut acl.owner, "acl", owners_change, &[("Owner", 1)]).await;

    let mut c = acl.as_principal(&bob()).await;
    let version = edit(&mut c, None, vec![set_edit("pub/b.rs", b"fn b() {}\n")])
        .await
        .unwrap();
    let change = change_of(&mut c, &version).await;
    c.review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    let mut rev = acl.as_principal(&rill_types::Principal::human("rev")).await;
    vote(&mut rev, &change, "Review", 2).await.unwrap();
    let allowed = |c: &mut Clients| {
        let review = c.review.clone();
        let change = change.clone();
        async move {
            review
                .evaluate_landing(pb::EvaluateLandingRequest {
                    change: common::names::change("acl", &change),
                })
                .await
                .unwrap()
                .into_owned()
                .decision
                .unwrap()
        }
    };
    let d = allowed(&mut rev).await;
    assert!(!d.allowed, "{:?}", d.reasons);
    assert!(d
        .matched_policies
        .contains(&"block_uncovered_owners".to_string()));

    let mut ann = acl.as_principal(&rill_types::Principal::human("ann")).await;
    vote(&mut ann, &change, "Owner", 1).await.unwrap();
    assert!(!allowed(&mut rev).await.allowed, "one of two required");
    assert_eq!(
        vote(&mut rev, &change, "Owner", 1).await,
        Err(Code::PermissionDenied),
        "not in the set"
    );
    let mut dan = acl.as_principal(&rill_types::Principal::human("dan")).await;
    vote(&mut dan, &change, "Owner", 1).await.unwrap();
    let d = allowed(&mut rev).await;
    assert!(d.allowed, "{:?}", d.reasons);
    acl.server.stop().await;
}

#[tokio::test]
async fn an_overdue_owner_review_escalates_to_the_contact() {
    let mut acl = Acl::start().await;
    add_user(&acl.server, "ann", vec![], &[]).await;
    acl.grant("pub", "writer", &bob()).await;
    let owners = edit(
        &mut acl.owner,
        None,
        vec![set_edit(
            "pub/OWNERS.toml",
            b"owners = [\"user:ann\"]\nsla = \"1s\"\nescalate_to = \"user:esc\"\n",
        )],
    )
    .await
    .unwrap();
    let owners_change = change_of(&mut acl.owner, &owners).await;
    land_with(&mut acl.owner, "acl", owners_change, &[("Owner", 1)]).await;

    let mut c = acl.as_principal(&bob()).await;
    let version = edit(&mut c, None, vec![set_edit("pub/b.rs", b"fn b() {}\n")])
        .await
        .unwrap();
    let change = change_of(&mut c, &version).await;
    c.review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let review = loop {
        let review = acl
            .owner
            .review
            .get_review(pb::GetReviewRequest {
                name: common::names::review("acl", &change),
            })
            .await
            .unwrap()
            .into_owned()
            .review
            .unwrap();
        if review.extra.contains_key("escalation:pub#default") {
            break review;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no escalation: {:?}",
            review.extra
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    };
    assert!(review.extra["escalation:pub#default"].starts_with("1|user:esc|"));
    let thread = review
        .threads
        .iter()
        .find(|t| t.to.as_option().is_some_and(|p| p.name == "esc"))
        .expect("a thread addressed to the escalation contact");
    assert!(thread.comments[0].body.contains("past its SLA"));
    acl.server.stop().await;
}
