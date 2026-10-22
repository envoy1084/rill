//! Intent inputs and search results.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{Intent, IntentId, IntentKind, IntentStatus, PrincipalId, RiskClass};

use crate::error::{Error, Result};

/// A three-state field update: leave alone, set, or clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldUpdate<T> {
    /// Leave the field as it is.
    #[default]
    Keep,
    /// Set the field.
    Set(T),
    /// Clear the field.
    Clear,
}

impl<T> FieldUpdate<T> {
    /// The value to send, if any.
    pub fn value(&self) -> Option<&T> {
        match self {
            FieldUpdate::Set(v) => Some(v),
            _ => None,
        }
    }

    /// True when the field should be cleared.
    pub fn clears(&self) -> bool {
        matches!(self, FieldUpdate::Clear)
    }
}

/// Inputs to create an intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewIntent {
    /// Title.
    pub title: String,
    /// Body text.
    pub body: String,
    /// Kind.
    pub kind: IntentKind,
    /// Parent intent.
    pub parent: Option<IntentId>,
    /// Owner-set risk override.
    pub risk_override: Option<RiskClass>,
}

impl NewIntent {
    /// An intent with a title and kind.
    pub fn new(title: impl Into<String>, kind: IntentKind) -> Self {
        NewIntent {
            title: title.into(),
            body: String::new(),
            kind,
            parent: None,
            risk_override: None,
        }
    }

    /// Set the body.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }
}

/// Partial update of an intent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntentUpdate {
    /// New title.
    pub title: Option<String>,
    /// New body.
    pub body: Option<String>,
    /// New status.
    pub status: Option<IntentStatus>,
    /// Parent change.
    pub parent: FieldUpdate<IntentId>,
    /// Risk override change.
    pub risk_override: FieldUpdate<RiskClass>,
}

/// Filters for listing intents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntentFilter {
    /// Only these statuses; empty means any.
    pub status: Vec<IntentStatus>,
    /// Only these kinds; empty means any.
    pub kind: Vec<IntentKind>,
    /// Only intents owned by this principal.
    pub owner: Option<PrincipalId>,
    /// Only children of this intent.
    pub parent: Option<IntentId>,
    /// Page size; `0` lets the server choose.
    pub limit: u32,
    /// Continue from a previous page.
    pub page_token: Option<String>,
}

/// A similarity search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredIntent {
    /// The intent.
    pub intent: Intent,
    /// Similarity score; higher is closer.
    pub score: f32,
}

impl TryFrom<pb::ScoredIntent> for ScoredIntent {
    type Error = Error;

    fn try_from(s: pb::ScoredIntent) -> Result<Self> {
        Ok(ScoredIntent {
            intent: required("scored_intent.intent", s.intent)?.try_into()?,
            score: s.score,
        })
    }
}
