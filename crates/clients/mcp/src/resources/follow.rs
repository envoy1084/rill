//! Resource updates: a change's diff and review resources change whenever
//! its review does, so subscriptions follow the review's event stream.

use std::collections::BTreeMap;

use futures::stream::{self, BoxStream, StreamExt};
use rill_types::ChangeId;

use super::uri::ResourceUri;
use crate::tools::ToolContext;

/// The URIs among `uris` that can be subscribed to.
pub fn watchable(uris: &[String]) -> Vec<String> {
    uris.iter()
        .filter(|u| {
            ResourceUri::parse(u)
                .ok()
                .is_some_and(|r| r.watched_change().is_some())
        })
        .cloned()
        .collect()
}

/// A stream yielding, after every review event, the URIs it updated.
/// Ends when every watched review's stream ends.
pub async fn updates(ctx: &ToolContext, uris: &[String]) -> BoxStream<'static, Vec<String>> {
    let mut by_change: BTreeMap<(String, ChangeId), Vec<String>> = BTreeMap::new();
    for uri in uris {
        if let Some((repo, change)) = ResourceUri::parse(uri)
            .ok()
            .as_ref()
            .and_then(ResourceUri::watched_change)
        {
            by_change
                .entry((repo.to_string(), change))
                .or_default()
                .push(uri.clone());
        }
    }
    let mut streams = Vec::new();
    for ((repo, change), uris) in by_change {
        match ctx.client().repo(repo).reviews().watch(change).await {
            Ok(events) => {
                streams.push(events.map(move |_| uris.clone()).boxed());
            }
            Err(e) => tracing::debug!(error = %e, %change, "cannot watch review"),
        }
    }
    stream::select_all(streams).boxed()
}
