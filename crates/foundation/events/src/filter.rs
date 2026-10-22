//! Which events a reader wants.

use rill_types::{ChangeId, Event, EventKind};

/// A conjunction of conditions; an empty filter matches every event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventFilter {
    /// Event kinds by [`EventKind::name`]; empty matches any kind.
    pub kinds: Vec<String>,
    /// Only events about this change.
    pub change: Option<ChangeId>,
    /// Only ref moves of refs starting with this prefix.
    pub ref_prefix: Option<String>,
}

impl EventFilter {
    /// Events of the named kinds.
    pub fn kinds<I: IntoIterator<Item = S>, S: Into<String>>(kinds: I) -> Self {
        EventFilter {
            kinds: kinds.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Whether `event` passes.
    pub fn matches(&self, event: &Event) -> bool {
        if !self.kinds.is_empty() && !self.kinds.iter().any(|k| k == event.kind.name()) {
            return false;
        }
        if let Some(change) = &self.change {
            if event.kind.change() != Some(*change) {
                return false;
            }
        }
        if let Some(prefix) = &self.ref_prefix {
            match &event.kind {
                EventKind::RefMoved { name, .. } if name.starts_with(prefix.as_str()) => {}
                _ => return false,
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use rill_types::{Principal, Timestamp};

    use super::*;

    fn event(kind: EventKind) -> Event {
        Event {
            kind,
            actor: Principal::human("a"),
            at: Timestamp::from_millis(1),
        }
    }

    #[test]
    fn conditions_all_have_to_hold() {
        let change = ChangeId::new();
        let moved = event(EventKind::RefMoved {
            name: "release/1".into(),
            old: None,
            new: None,
        });
        let status = event(EventKind::StatusChanged {
            change,
            from: rill_types::ReviewStatus::Open,
            to: rill_types::ReviewStatus::Landable,
        });

        assert!(EventFilter::default().matches(&moved));
        let refs = EventFilter {
            ref_prefix: Some("release/".into()),
            ..EventFilter::default()
        };
        assert!(refs.matches(&moved));
        assert!(!refs.matches(&status), "a ref filter only passes ref moves");

        let of_change = EventFilter {
            change: Some(change),
            ..EventFilter::kinds(["status_changed"])
        };
        assert!(of_change.matches(&status));
        assert!(!of_change.matches(&moved));
        let other = EventFilter {
            change: Some(ChangeId::new()),
            ..EventFilter::default()
        };
        assert!(!other.matches(&status));
    }
}
