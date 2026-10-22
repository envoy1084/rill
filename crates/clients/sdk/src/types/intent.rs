//! Intent inputs and search results.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{Intent, IntentId, IntentKind, IntentStatus, RiskClass};

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
    /// Only intents owned by this principal, as shown (`ann`).
    pub owner: Option<String>,
    /// Only children of this intent.
    pub parent: Option<IntentId>,
    /// Page size; `0` lets the server choose.
    pub limit: u32,
    /// Continue from a previous page.
    pub page_token: Option<String>,
}

impl IntentFilter {
    /// The filter text the server takes.
    pub fn to_filter(&self) -> String {
        let statuses: Vec<&str> = self
            .status
            .iter()
            .map(|s| match s {
                IntentStatus::Open => "open",
                IntentStatus::InProgress => "in_progress",
                IntentStatus::Done => "done",
                IntentStatus::Abandoned => "abandoned",
            })
            .collect();
        let kinds: Vec<String> = self
            .kind
            .iter()
            .map(|k| format!("{k:?}").to_lowercase())
            .collect();
        crate::filter::FilterBuilder::new()
            .any_of("status", &statuses)
            .any_of("kind", &kinds)
            .eq_opt("owner", self.owner.as_deref())
            .eq_opt("parent", self.parent.map(|p| p.to_string()))
            .build()
    }
}

impl IntentUpdate {
    /// The intent fields and the mask naming them.
    pub(crate) fn to_proto(&self, name: String) -> (pb::Intent, Vec<String>) {
        let mut mask = Vec::new();
        let mut intent = pb::Intent {
            name,
            ..Default::default()
        };
        if let Some(title) = &self.title {
            intent.title = title.clone();
            mask.push("title".to_string());
        }
        if let Some(body) = &self.body {
            intent.body = body.clone();
            mask.push("body".to_string());
        }
        if let Some(status) = self.status {
            intent.status = pb::IntentStatus::from(status).into();
            mask.push("status".to_string());
        }
        if self.parent != FieldUpdate::Keep {
            intent.parent = self.parent.value().map(rill_proto::convert::id_to_proto);
            mask.push("parent".to_string());
        }
        if self.risk_override != FieldUpdate::Keep {
            intent.risk_override = self
                .risk_override
                .value()
                .map(|r| pb::RiskClass::from(*r).into());
            mask.push("risk_override".to_string());
        }
        (intent, mask)
    }
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
