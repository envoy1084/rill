//! Finding intents like a title and body, to catch duplicates.

use std::collections::BTreeSet;

use rill_proto::rill::v1 as pb;
use rill_types::{Intent, RepoPathBuf};

use super::records::intent_proto;
use super::IntentSvc;
use crate::services::common::open_repo;
use crate::services::Call;
use rill_runtime::error::Result;

impl IntentSvc {
    /// Token-overlap (Jaccard) similarity over title and body. Good
    /// enough to catch duplicates with shared vocabulary; embeddings are
    /// the upgrade path when a vector index is available.
    pub(super) async fn find_similar_intents_impl(
        &self,
        request: Call<pb::FindSimilarIntentsRequest>,
    ) -> Result<pb::FindSimilarIntentsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let query = tokens(&format!("{} {}", req.title, req.body));
        if query.is_empty() {
            return Ok(pb::FindSimilarIntentsResponse::default());
        }
        let limit = match req.max_results {
            n if n <= 0 => 10,
            n => (n as usize).min(100),
        };
        let mut scored: Vec<(f32, Intent)> = opened
            .repo
            .repo
            .list_intents()
            .await?
            .into_iter()
            .filter_map(|intent| {
                let score = jaccard(
                    &query,
                    &tokens(&format!("{} {}", intent.title, intent.body)),
                );
                (score > 0.0).then_some((score, intent))
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(pb::FindSimilarIntentsResponse {
            results: scored
                .into_iter()
                .map(|(score, intent)| pb::ScoredIntent {
                    intent: intent_proto(&opened.repo, intent).into(),
                    score,
                })
                .collect(),
        })
    }
}

/// Lower-cased alphanumeric words of at least three characters.
fn tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_lowercase)
        .collect()
}

fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f32 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f32 / union as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_scores_overlap() {
        let a = tokens("Fix login timeout on slow networks");
        let b = tokens("Login timeout fix for slow network");
        let c = tokens("Add dark mode");
        assert!(jaccard(&a, &b) > jaccard(&a, &c));
        assert_eq!(jaccard(&a, &c), 0.0);
        assert_eq!(jaccard(&a, &a), 1.0);
    }
}
