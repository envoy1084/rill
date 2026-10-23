//! Policy as code: trunk's `.rill/policy/` rules take over from the stored
//! policy, every landing records the decision behind it, candidates replay
//! against that history, and org guardrails forbid above it all.

use super::super::*;
use super::fixture::*;
use super::writes::{change_of, edit};

async fn effective(c: &mut Clients) -> pb::GetEffectivePolicyResponse {
    c.policy
        .get_effective_policy(pb::GetEffectivePolicyRequest {
            parent: common::names::repo("acl"),
        })
        .await
        .unwrap()
        .into_owned()
}

async fn land_files(acl: &mut Acl, files: &[(&str, &str)]) -> u64 {
    let edits = files
        .iter()
        .map(|(p, t)| set_edit(p, t.as_bytes()))
        .collect();
    let v = edit(&mut acl.owner, None, edits).await.unwrap();
    let change = change_of(&mut acl.owner, &v).await;
    land_with(&mut acl.owner, "acl", change, &[("Owner", 1)]).await
}

async fn wait_for<T>(mut probe: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if let Some(v) = probe().await {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

const REPO_RULES: &str = r#"@id("repo_owner_and_human")
permit(principal, action == Rill::Action::"land", resource) when {
    resource.hasTag("Owner") && resource.getTag("Owner").human_max >= 1
    && resource.hasTag("Review") && resource.getTag("Review").human_max >= 2
};
"#;

#[tokio::test]
async fn trunk_rules_record_decisions_replay_and_yield_to_guardrails() {
    let mut acl = Acl::start().await;
    assert_eq!(effective(&mut acl.owner).await.source, "default");
    assert_eq!(
        edit(
            &mut acl.owner,
            None,
            vec![set_edit(".rill/policy/bad.cedar", b"permit(principal")]
        )
        .await,
        Err(Code::InvalidArgument),
        "a policy file that does not compile is refused"
    );
    land_files(&mut acl, &[(".rill/policy/submit.cedar", REPO_RULES)]).await;
    let now = wait_for(async || {
        let e = effective(&mut acl.owner).await;
        e.source.contains(".rill/policy/submit.cedar").then_some(e)
    })
    .await;
    assert!(now.text.contains("repo_owner_and_human"));

    let seq = land_files(&mut acl, &[("pub/b.rs", "fn b() {}\n")]).await;
    let landing = common::names::position("acl", seq);
    let why = wait_for(async || {
        acl.owner
            .policy
            .explain_landing(pb::ExplainLandingRequest {
                landing: landing.clone(),
            })
            .await
            .ok()
            .map(|r| r.into_owned())
    })
    .await;
    assert!(why.allowed);
    assert_eq!(why.matched_policies, ["repo_owner_and_human"]);
    assert_eq!(why.policy, now.digest);
    assert!(why.policy_text.contains("repo_owner_and_human"));

    let sim = acl
        .owner
        .policy
        .simulate_policy(pb::SimulatePolicyRequest {
            parent: common::names::repo("acl"),
            text: r#"@id("needs_two_owners") permit(principal, action, resource) when {
                resource.hasTag("Owner") && resource.getTag("Owner").human_approvals >= 2 };"#
                .into(),
            since: seq,
            until: seq,
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!((sim.replayed, sim.allowed), (1, 0));
    assert_eq!(sim.blocked[0].landing, landing);

    let guard = r#"@id("org_nothing_lands") forbid(principal, action, resource);"#;
    let olga = add_user(
        &acl.server,
        "olga",
        vec![rill_types::identity::OrgRole::Admin],
        &[],
    )
    .await;
    let admin = acl.as_principal(&olga).await;
    admin
        .policy
        .update_guardrails(pb::UpdateGuardrailsRequest {
            org: common::names::org("default"),
            text: guard.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    wait_for(async || {
        effective(&mut acl.owner)
            .await
            .source
            .ends_with("+ org guardrails")
            .then_some(())
    })
    .await;
    let v = edit(
        &mut acl.owner,
        None,
        vec![set_edit("pub/c.rs", b"fn c() {}\n")],
    )
    .await
    .unwrap();
    let change = change_of(&mut acl.owner, &v).await;
    acl.owner
        .review
        .submit_change(pb::SubmitChangeRequest {
            request_id: String::new(),
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap();
    let d = acl
        .owner
        .review
        .evaluate_landing(pb::EvaluateLandingRequest {
            change: common::names::change("acl", &change),
        })
        .await
        .unwrap()
        .into_owned()
        .decision
        .unwrap();
    assert!(!d.allowed);
    assert_eq!(d.matched_policies, ["org_nothing_lands"]);
    let bob = acl.as_principal(&bob()).await;
    let denied = bob
        .policy
        .update_guardrails(pb::UpdateGuardrailsRequest {
            org: common::names::org("default"),
            text: String::new(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(denied.code, Code::PermissionDenied);
    acl.server.stop().await;
}
