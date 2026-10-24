//! CPU-bound query work, run on rayon's fixed pool rather than as many
//! blocking tasks: a query fans out over every shard, and thousands of
//! queries a second spawning blocking tasks contended on the blocking
//! pool's lock and grew hundreds of threads.
//!
//! Each item is its own pool job answering on a channel. A pool thread
//! never waits for another job (a parallel `collect` would): a waiting
//! worker runs queued jobs on its own stack, and with thousands of
//! queries queued that nesting overflowed it.

use std::sync::Arc;

use crate::error::{Error, Result};

/// Run `f` over `items` in parallel on the pool, and await the results in
/// order.
pub async fn map<I, T, F>(items: Vec<I>, f: F) -> Result<Vec<T>>
where
    I: Send + 'static,
    T: Send + 'static,
    F: Fn(I) -> Result<T> + Send + Sync + 'static,
{
    let f = Arc::new(f);
    let answers: Vec<_> = items
        .into_iter()
        .map(|item| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let f = f.clone();
            rayon::spawn(move || {
                let _ = tx.send(f(item));
            });
            rx
        })
        .collect();
    let mut out = Vec::with_capacity(answers.len());
    for rx in answers {
        out.push(rx.await.map_err(|_| Error::Worker)??);
    }
    Ok(out)
}
