//! One origin fetch per object however many requests miss on it at once.
//! A request waiting on another's fetch takes its result when it succeeds,
//! and fetches for itself when it fails: the leader's failure may be its
//! own credentials.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use tokio::sync::watch;

/// `None` until the leader finishes, then its value, or `Some(None)` when it
/// failed. A leader that goes away drops the sender, which waiters see too.
type Slot<V> = watch::Receiver<Option<Option<V>>>;

/// In-flight fetches by key.
pub struct Coalescer<K, V> {
    inflight: Mutex<HashMap<K, Slot<V>>>,
}

impl<K: Eq + Hash + Clone, V: Clone> Coalescer<K, V> {
    pub fn new() -> Self {
        Coalescer {
            inflight: Mutex::new(HashMap::new()),
        }
    }

    /// `fetch`'s result for `key`, shared with concurrent callers. Returns
    /// whether this call waited on another's fetch.
    pub async fn get<E, F, Fut>(&self, key: K, fetch: F) -> (Result<V, E>, bool)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        let waiting = {
            let mut inflight = self.inflight.lock();
            match inflight.get(&key) {
                Some(slot) => Err(slot.clone()),
                None => {
                    let (tx, rx) = watch::channel(None);
                    inflight.insert(key.clone(), rx);
                    Ok(tx)
                }
            }
        };
        let mut slot = match waiting {
            Ok(tx) => {
                let result = fetch().await;
                self.inflight.lock().remove(&key);
                let _ = tx.send(Some(result.as_ref().ok().cloned()));
                return (result, false);
            }
            Err(slot) => slot,
        };
        let shared = slot
            .wait_for(|v| v.is_some())
            .await
            .ok()
            .and_then(|v| v.clone())
            .flatten();
        match shared {
            Some(value) => (Ok(value), true),
            None => (fetch().await, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn concurrent_misses_share_one_fetch() {
        let coalescer = Arc::new(Coalescer::<u8, u32>::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..50 {
            let (coalescer, calls) = (coalescer.clone(), calls.clone());
            tasks.push(tokio::spawn(async move {
                coalescer
                    .get(1, || {
                        let calls = calls.clone();
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            Ok::<_, ()>(7)
                        }
                    })
                    .await
                    .0
            }));
        }
        for t in tasks {
            assert_eq!(t.await.unwrap(), Ok(7));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_failed_fetch_is_not_shared() {
        let coalescer = Arc::new(Coalescer::<u8, u32>::new());
        let leader = {
            let coalescer = coalescer.clone();
            tokio::spawn(async move {
                coalescer
                    .get(1, || async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Err::<u32, &str>("leader's token was bad")
                    })
                    .await
                    .0
            })
        };
        tokio::time::sleep(Duration::from_millis(10)).await;
        let follower = coalescer.get(1, || async { Ok::<_, &str>(9) }).await.0;
        assert_eq!(follower, Ok(9));
        assert!(leader.await.unwrap().is_err());
    }
}
