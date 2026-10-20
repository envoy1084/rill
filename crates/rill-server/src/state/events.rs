//! Per-repository event bus.
//!
//! Review events, landing events and ref moves fan into one broadcast
//! channel so the `Watch*` RPCs have a single thing to subscribe to. The
//! channel is lossy by design (`tokio::sync::broadcast`): a slow watcher
//! that lags is told how many events it missed and re-syncs from the
//! store, which is the source of truth.

use rill_lander::LandEvent;
use rill_review::service::ReviewEvent;
use rill_types::{ChangeVersionHash, RefName, Timestamp};
use tokio::sync::broadcast;

/// Capacity of the per-repo broadcast channel.
pub const EVENT_CAPACITY: usize = 4096;

/// Anything that happened to a repository that a watcher may care about.
#[derive(Debug, Clone)]
pub enum RepoEvent {
    /// The review state machine moved.
    Review(ReviewEvent),
    /// The landing pipeline moved.
    Land(LandEvent),
    /// A ref was created, moved or deleted (including `trunk` on landing).
    RefMoved {
        /// The ref.
        name: RefName,
        /// Previous target, if any.
        old: Option<ChangeVersionHash>,
        /// New target; `None` when deleted.
        new: Option<ChangeVersionHash>,
        /// When the move happened.
        at: Timestamp,
    },
    /// A change version was written to the repository.
    VersionCreated {
        /// Its hash.
        hash: ChangeVersionHash,
    },
}

/// A cloneable handle to a repository's broadcast channel.
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<RepoEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// A bus with the default capacity.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CAPACITY);
        EventBus { sender }
    }

    /// Publish an event. Dropped silently when nobody is listening.
    pub fn publish(&self, event: RepoEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to events published from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<RepoEvent> {
        self.sender.subscribe()
    }

    /// Number of live subscribers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Forward every event from `rx` onto this bus until `rx` closes.
    /// Lagged receivers simply skip; the watchers re-sync from the store.
    pub fn forward<T, F>(
        &self,
        mut rx: broadcast::Receiver<T>,
        map: F,
    ) -> tokio::task::JoinHandle<()>
    where
        T: Clone + Send + 'static,
        F: Fn(T) -> Vec<RepoEvent> + Send + 'static,
    {
        let bus = self.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        for mapped in map(event) {
                            bus.publish(mapped);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "event forwarder lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }
}

/// Map a landing event onto bus events: the event itself, plus a `trunk`
/// ref move for landings so ref watchers see trunk advance.
pub fn land_to_bus(event: LandEvent) -> Vec<RepoEvent> {
    let mut out = Vec::with_capacity(2);
    if let LandEvent::Landed { version, .. } = &event {
        out.push(RepoEvent::RefMoved {
            name: RefName::trunk(),
            old: None,
            new: Some(*version),
            at: Timestamp::now(),
        });
    }
    out.push(RepoEvent::Land(event));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ChangeId, ObjectHash, TrunkSeq};

    #[tokio::test]
    async fn forwards_and_expands_land_events() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let (tx, lander_rx) = broadcast::channel(4);
        let _task = bus.forward(lander_rx, land_to_bus);
        tx.send(LandEvent::Landed {
            seq: TrunkSeq(1),
            change_id: ChangeId::new(),
            version: ObjectHash::ZERO,
        })
        .unwrap();
        assert!(matches!(
            rx.recv().await.unwrap(),
            RepoEvent::RefMoved { .. }
        ));
        assert!(matches!(rx.recv().await.unwrap(), RepoEvent::Land(_)));
    }
}
