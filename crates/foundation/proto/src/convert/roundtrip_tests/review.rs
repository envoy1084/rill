//! Review state and labels.

use std::collections::BTreeMap;

use rill_types::objects::review::CommentCategory;
use rill_types::*;

use super::fixtures::{h, p, wire};
use crate::error::ConvertError;
use crate::rill::v1 as pb;

#[test]
fn review_state_roundtrip() {
    let state = ReviewState {
        change_id: ChangeId::derived("test", "cv"),
        versions: vec![h("v1"), h("v2")],
        status: ReviewStatus::Landable,
        intent: Some(IntentId::derived("test", "i")),
        owner: Principal::human("alice"),
        votes: vec![LabelVote {
            label: "Review".into(),
            value: -2,
            voter: Principal::agent("reviewer"),
            version: h("v2"),
            voted_at: Timestamp::from_millis(10),
            reason: Some("nope".into()),
        }],
        threads: vec![
            Thread {
                id: "t1".into(),
                anchor: ThreadAnchor::Lines {
                    version: h("v2"),
                    path: p("src/lib.rs"),
                    start: 3,
                    end: 7,
                    context_hash: "abc".into(),
                },
                comments: vec![Comment {
                    author: Principal::agent("reviewer"),
                    body: "bug".into(),
                    created_at: Timestamp::from_millis(11),
                    category: Some(CommentCategory::Bug),
                    confidence: Some(87),
                    suggestion: None,
                }],
                resolved: false,
                to: Some(Principal::human("alice")),
                outdated: false,
            },
            Thread {
                id: "t2".into(),
                anchor: ThreadAnchor::Change,
                comments: vec![Comment {
                    author: Principal::human("alice"),
                    body: "lgtm".into(),
                    created_at: Timestamp::from_millis(12),
                    category: None,
                    confidence: None,
                    suggestion: None,
                }],
                resolved: true,
                to: None,
                outdated: false,
            },
            Thread {
                id: "t3".into(),
                anchor: ThreadAnchor::File {
                    version: h("v1"),
                    path: p("docs/x.md"),
                },
                comments: vec![],
                resolved: false,
                to: None,
                outdated: false,
            },
        ],
        risk: RiskClass::Sensitive,
        risk_reasons: vec!["touches auth".into()],
        stack: vec![
            ChangeId::derived("test", "a"),
            ChangeId::derived("test", "cv"),
        ],
        landed_seq: None,
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(2),
        extra: BTreeMap::from([("k".to_string(), "v".to_string())]),
    };
    assert_eq!(
        ReviewState::try_from(wire(pb::ReviewState::from(&state))).unwrap(),
        state
    );

    let mut bad = pb::ReviewState::from(&state);
    bad.votes[0].value = 1000;
    assert_eq!(
        ReviewState::try_from(bad).unwrap_err(),
        ConvertError::OutOfRange {
            field: "label_vote.value",
            value: 1000
        }
    );
    let mut bad = pb::ReviewState::from(&state);
    bad.threads[0].comments[0].confidence = Some(101);
    assert!(matches!(
        ReviewState::try_from(bad).unwrap_err(),
        ConvertError::OutOfRange {
            field: "comment.confidence",
            ..
        }
    ));
}

#[test]
fn label_roundtrip() {
    for label in [Label::review(), Label::verified(), Label::owner()] {
        assert_eq!(
            Label::try_from(wire(pb::Label::from(&label))).unwrap(),
            label
        );
    }
}
