//! What one MCP session remembers between calls: the change it is working
//! on and the last failing call. Stdio has one session per process; over
//! HTTP every session gets its own, and stateless requests pass `change`.

use std::collections::HashMap;
use std::sync::Mutex;

use rill_types::ChangeId;
use tokio::task::AbortHandle;

use crate::tools::RepeatGuard;

/// Per-session state shared by every tool call in the session.
#[derive(Debug, Default)]
pub struct Session {
    current: Mutex<Option<(String, ChangeId)>>,
    guard: Mutex<RepeatGuard>,
    subscriptions: Mutex<HashMap<String, AbortHandle>>,
}

impl Session {
    /// The change being worked on in `repo`, if any.
    pub fn current(&self, repo: &str) -> Option<ChangeId> {
        lock(&self.current)
            .as_ref()
            .filter(|(r, _)| r == repo)
            .map(|(_, id)| *id)
    }

    /// Make `id` the change being worked on in `repo`.
    pub fn set_current(&self, repo: &str, id: ChangeId) {
        *lock(&self.current) = Some((repo.to_string(), id));
    }

    /// Forget `id` once it has landed or been abandoned.
    pub fn clear(&self, id: ChangeId) {
        let mut current = lock(&self.current);
        if current.as_ref().is_some_and(|(_, c)| *c == id) {
            *current = None;
        }
    }

    /// Run `f` on the repeated-failure guard.
    pub fn guard<T>(&self, f: impl FnOnce(&mut RepeatGuard) -> T) -> T {
        f(&mut lock(&self.guard))
    }
}

impl Session {
    /// Keep `task` delivering updates for `uri` until unsubscribed,
    /// replacing any earlier subscription to it.
    pub fn subscribe(&self, uri: String, task: AbortHandle) {
        if let Some(old) = lock(&self.subscriptions).insert(uri, task) {
            old.abort();
        }
    }

    /// Stop delivering updates for `uri`.
    pub fn unsubscribe(&self, uri: &str) {
        if let Some(task) = lock(&self.subscriptions).remove(uri) {
            task.abort();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for task in lock(&self.subscriptions).values() {
            task.abort();
        }
    }
}

/// Locks are held only for a field update, never across an await, so a
/// poisoned lock still holds a consistent value.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
