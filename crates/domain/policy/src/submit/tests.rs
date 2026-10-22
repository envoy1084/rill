use rill_types::objects::review::CommentCategory;
use rill_types::{
    ChangeId, Comment, LabelVote, ObjectHash, Principal, RepoPathBuf, ReviewState, ReviewStatus,
    RiskClass, Thread, ThreadAnchor, Timestamp,
};

use super::*;
use crate::risk::{ChangeFacts, PathFacts};

fn change() -> ChangeFacts {
    ChangeFacts::new(
        Principal::agent("coder"),
        vec![PathFacts::modified(RepoPathBuf::new("src/lib.rs").unwrap())],
    )
}

fn human_review(value: i8) -> VoteSummary {
    VoteSummary {
        max: value,
        min: value.min(0),
        human_max: value,
        human_votes: 1,
        human_approvals: u32::from(value > 0),
        ..Default::default()
    }
}

fn agent_approvals(n: u32) -> VoteSummary {
    VoteSummary {
        max: 1,
        agent_max: 1,
        agent_votes: n,
        agent_approvals: n,
        ..Default::default()
    }
}

fn verified() -> VoteSummary {
    VoteSummary {
        max: 1,
        agent_max: 1,
        agent_votes: 1,
        agent_approvals: 1,
        ..Default::default()
    }
}

fn eval(facts: SubmitFacts) -> SubmitDecision {
    SubmitPolicy::default_rules()
        .evaluate(&Principal::human("lander"), &change(), &facts)
        .unwrap()
}

#[test]
fn default_policy_validates() {
    let p = SubmitPolicy::default_rules();
    assert_eq!(p.len(), 6);
}

#[test]
fn trivial_lands_on_verified() {
    for risk in [RiskClass::Trivial, RiskClass::Low] {
        let d = eval(SubmitFacts::new(risk).with_label("Verified", verified()));
        assert!(d.allowed, "{risk}: {:?}", d.reasons);
        assert_eq!(d.matched_policies, vec!["land_trivial_or_low"]);
    }
}

#[test]
fn trivial_without_verified_is_blocked() {
    let d = eval(SubmitFacts::new(RiskClass::Trivial));
    assert!(!d.allowed);
    assert!(d.matched_policies.is_empty());
    assert_eq!(d.reasons, vec!["no permit rule matched"]);
}

#[test]
fn trivial_with_minus_two_is_blocked() {
    let d = eval(
        SubmitFacts::new(RiskClass::Trivial)
            .with_label("Verified", verified())
            .with_label("Review", human_review(-2)),
    );
    assert!(!d.allowed);
    assert_eq!(d.matched_policies, vec!["block_review_veto"]);
}

#[test]
fn normal_needs_human_plus_two() {
    let d = eval(SubmitFacts::new(RiskClass::Normal).with_label("Review", human_review(2)));
    assert!(d.allowed);
    assert_eq!(d.matched_policies, vec!["land_normal_human_review"]);

    let d = eval(SubmitFacts::new(RiskClass::Normal).with_label("Review", human_review(1)));
    assert!(!d.allowed);

    // Verified alone is not enough for normal risk.
    let d = eval(SubmitFacts::new(RiskClass::Normal).with_label("Verified", verified()));
    assert!(!d.allowed);
}

#[test]
fn normal_accepts_two_agents_plus_human_owner() {
    let d = eval(
        SubmitFacts::new(RiskClass::Normal)
            .with_label("Review", agent_approvals(2))
            .with_label("Owner", human_review(1)),
    );
    assert!(d.allowed, "{:?}", d.reasons);
    assert_eq!(
        d.matched_policies,
        vec!["land_normal_agent_review_with_owner"]
    );

    // One agent is not enough.
    let d = eval(
        SubmitFacts::new(RiskClass::Normal)
            .with_label("Review", agent_approvals(1))
            .with_label("Owner", human_review(1)),
    );
    assert!(!d.allowed);

    // Two agents without an owner is not enough.
    let d = eval(SubmitFacts::new(RiskClass::Normal).with_label("Review", agent_approvals(2)));
    assert!(!d.allowed);

    // An agent "owner" vote does not count.
    let owner_by_agent = VoteSummary {
        max: 1,
        agent_max: 1,
        agent_votes: 1,
        agent_approvals: 1,
        ..Default::default()
    };
    let d = eval(
        SubmitFacts::new(RiskClass::Normal)
            .with_label("Review", agent_approvals(2))
            .with_label("Owner", owner_by_agent),
    );
    assert!(!d.allowed);
}

#[test]
fn sensitive_and_critical_need_owner_and_human_review() {
    for risk in [RiskClass::Sensitive, RiskClass::Critical] {
        let ok = eval(
            SubmitFacts::new(risk)
                .with_label("Review", human_review(2))
                .with_label("Owner", human_review(1)),
        );
        assert!(ok.allowed, "{risk}: {:?}", ok.reasons);
        assert_eq!(ok.matched_policies, vec!["land_sensitive_or_critical"]);

        let no_owner = eval(SubmitFacts::new(risk).with_label("Review", human_review(2)));
        assert!(!no_owner.allowed, "{risk} without owner");

        let agents_only = eval(
            SubmitFacts::new(risk)
                .with_label("Review", agent_approvals(2))
                .with_label("Owner", human_review(1)),
        );
        assert!(!agents_only.allowed, "{risk} with agent review only");
    }
}

#[test]
fn review_minus_two_blocks_everything() {
    let veto = VoteSummary {
        max: 2,
        min: -2,
        human_max: 2,
        human_votes: 2,
        human_approvals: 1,
        ..Default::default()
    };
    let d = eval(
        SubmitFacts::new(RiskClass::Sensitive)
            .with_label("Review", veto)
            .with_label("Owner", human_review(1)),
    );
    assert!(!d.allowed);
    assert_eq!(d.matched_policies, vec!["block_review_veto"]);
    assert!(d.reasons[0].starts_with("block_review_veto: forbid"));
}

#[test]
fn blocking_threads_block_but_discussion_does_not() {
    let base = SubmitFacts::new(RiskClass::Normal).with_label("Review", human_review(2));
    let d = eval(base.clone().with_threads(3, 0));
    assert!(d.allowed);
    let d = eval(base.with_threads(3, 1));
    assert!(!d.allowed);
    assert_eq!(d.matched_policies, vec!["block_unresolved_bug_or_security"]);
}

#[test]
fn invalid_policy_text_is_rejected() {
    assert!(matches!(
        SubmitPolicy::from_source("permit(principal"),
        Err(crate::Error::PolicyParse(_))
    ));
    // Type-checks fail: `risk` is a String, not a Long.
    let bad = r#"permit(principal, action == Rill::Action::"land", resource)
                 when { resource.risk > 3 };"#;
    assert!(matches!(
        SubmitPolicy::from_source(bad),
        Err(crate::Error::PolicyValidation(_))
    ));
    // Unknown attribute.
    let bad = r#"permit(principal, action == Rill::Action::"land", resource)
                 when { resource.nope };"#;
    assert!(matches!(
        SubmitPolicy::from_source(bad),
        Err(crate::Error::PolicyValidation(_))
    ));
}

#[test]
fn custom_policy_can_use_principal_and_change_attrs() {
    let src = r#"
        @id("agents_cannot_land_experiments")
        forbid(principal, action == Rill::Action::"land", resource)
        when { principal.kind == "agent" && resource.intent_kind == "experiment" };

        @id("humans_land_anything")
        permit(principal, action == Rill::Action::"land", resource)
        when { principal.kind == "human" };

        @id("agents_land_non_sensitive")
        permit(principal, action == Rill::Action::"land", resource)
        when { principal.kind == "agent" && !resource.touches_sensitive
               && !resource.has_agent_resolution };
    "#;
    let policy = SubmitPolicy::from_source(src).unwrap();
    let mut facts = SubmitFacts::new(RiskClass::Normal);
    let agent = Principal::agent("lander-bot");

    let d = policy.evaluate(&agent, &change(), &facts).unwrap();
    assert!(d.allowed);

    facts.touches_sensitive = true;
    let d = policy.evaluate(&agent, &change(), &facts).unwrap();
    assert!(!d.allowed);
    let d = policy
        .evaluate(&Principal::human("alice"), &change(), &facts)
        .unwrap();
    assert!(d.allowed);
}

#[test]
fn facts_from_review_state() {
    let alice = Principal::human("alice");
    let v1 = ObjectHash::digest(rill_types::ObjectKind::Derived, b"v1");
    let v2 = ObjectHash::digest(rill_types::ObjectKind::Derived, b"v2");
    let vote = |label: &str, value: i8, voter: Principal, version| LabelVote {
        label: label.into(),
        value,
        voter,
        version,
        voted_at: Timestamp::from_millis(0),
        reason: None,
    };
    let thread = |resolved: bool, category| Thread {
        id: "t".into(),
        anchor: ThreadAnchor::Change,
        comments: vec![Comment {
            author: alice.clone(),
            body: "x".into(),
            created_at: Timestamp::from_millis(0),
            category,
            confidence: None,
        }],
        resolved,
        to: None,
    };
    let state = ReviewState {
        change_id: ChangeId::derived("t", "c"),
        versions: vec![v1, v2],
        status: ReviewStatus::Open,
        intent: None,
        owner: alice.clone(),
        votes: vec![
            // Stale vote on v1 must be ignored.
            vote("Review", 2, alice.clone(), v1),
            vote("Review", 1, alice.clone(), v2),
            vote("Verified", 1, Principal::service("ci"), v2),
        ],
        threads: vec![
            thread(false, Some(CommentCategory::Security)),
            thread(false, Some(CommentCategory::Style)),
            thread(true, Some(CommentCategory::Bug)),
        ],
        risk: RiskClass::Sensitive,
        risk_reasons: vec![format!(
            "{}src/billing/x.rs matches \"src/billing/**\"",
            crate::risk::SENSITIVE_REASON_PREFIX
        )],
        stack: vec![],
        landed_seq: None,
        created_at: Timestamp::from_millis(0),
        updated_at: Timestamp::from_millis(0),
        extra: Default::default(),
    };
    let f = SubmitFacts::from_review(&state);
    assert_eq!(f.risk, RiskClass::Sensitive);
    assert_eq!(f.labels["Review"].human_max, 1);
    assert_eq!(f.labels["Verified"].max, 1);
    assert_eq!(f.labels["Verified"].human_votes, 0);
    assert_eq!(f.unresolved_threads, 2);
    assert_eq!(f.unresolved_blocking_threads, 1);
    assert!(f.touches_sensitive);
}
