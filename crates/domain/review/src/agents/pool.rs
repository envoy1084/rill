//! Fanning a change out to registered reviewers and recording what they
//! say.

use std::sync::Arc;
use std::time::Duration;

use rill_types::objects::review::CommentCategory;
use rill_types::{ChangeId, ChangeVersionHash, Principal, ThreadAnchor};

use super::reviewer::{Budget, Finding, ReviewContext, Reviewer};
use crate::error::{Error, Result};
use crate::service::ReviewService;

/// The label reviewers vote on.
pub const REVIEW_LABEL: &str = "Review";

/// Thresholds and limits for a [`ReviewerPool`].
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Per-reviewer budget.
    pub budget: Budget,
    /// Wall-clock limit for the whole pool run.
    pub total_timeout: Duration,
    /// Findings below this confidence are not posted.
    pub min_confidence_to_post: u8,
    /// A finding in a blocking category at or above this confidence turns
    /// the reviewer's vote negative.
    pub block_confidence: u8,
    /// Categories that can block.
    pub block_categories: Vec<CommentCategory>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            budget: Budget::default(),
            total_timeout: Duration::from_secs(300),
            min_confidence_to_post: 30,
            block_confidence: 70,
            block_categories: vec![CommentCategory::Bug, CommentCategory::Security],
        }
    }
}

impl PoolConfig {
    /// Whether a finding is severe enough to vote against the change.
    pub fn blocks(&self, finding: &Finding) -> bool {
        self.block_categories.contains(&finding.category)
            && finding.confidence >= self.block_confidence
    }
}

/// What one reviewer did during a run.
#[derive(Debug, Clone)]
pub struct ReviewerOutcome {
    /// The reviewer's name.
    pub name: String,
    /// Findings returned (posted or not).
    pub findings: Vec<Finding>,
    /// Ids of the threads opened for posted findings.
    pub thread_ids: Vec<String>,
    /// The `Review` vote cast, if any.
    pub vote: Option<i8>,
    /// Why the reviewer produced no vote, if it failed or timed out.
    pub error: Option<String>,
}

/// Result of a pool run.
#[derive(Debug, Clone)]
pub struct PoolReport {
    /// The change reviewed.
    pub change_id: ChangeId,
    /// The version the context was built from.
    pub version: ChangeVersionHash,
    /// One outcome per registered reviewer, in registration order.
    pub outcomes: Vec<ReviewerOutcome>,
    /// Whether the context was truncated to fit the token budget.
    pub truncated: bool,
}

/// A set of reviewers run together against a change.
#[derive(Default)]
pub struct ReviewerPool {
    reviewers: Vec<Arc<dyn Reviewer>>,
    config: PoolConfig,
}

impl ReviewerPool {
    /// An empty pool with `config`.
    pub fn new(config: PoolConfig) -> Self {
        ReviewerPool {
            reviewers: Vec::new(),
            config,
        }
    }

    /// Add a reviewer.
    pub fn register(&mut self, reviewer: Arc<dyn Reviewer>) -> &mut Self {
        self.reviewers.push(reviewer);
        self
    }

    /// Builder-style [`register`](Self::register).
    pub fn with(mut self, reviewer: Arc<dyn Reviewer>) -> Self {
        self.reviewers.push(reviewer);
        self
    }

    /// The configuration.
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// Names of the registered reviewers.
    pub fn names(&self) -> Vec<&str> {
        self.reviewers.iter().map(|r| r.name()).collect()
    }

    /// Review the change's current version with every reviewer, post the
    /// findings as threads and record each reviewer's `Review` vote.
    ///
    /// Reviewers run concurrently, each under the per-reviewer timeout and
    /// all under the total timeout. One reviewer failing does not stop the
    /// others; its outcome carries the error instead of a vote.
    pub async fn run(&self, service: &ReviewService, change_id: &ChangeId) -> Result<PoolReport> {
        let ctx = Arc::new(
            service
                .review_context(change_id, self.config.budget)
                .await?,
        );
        let results = self.fan_out(ctx.clone()).await;

        let mut outcomes = Vec::with_capacity(results.len());
        for (reviewer, result) in self.reviewers.iter().zip(results) {
            let name = reviewer.name().to_string();
            let outcome = match result {
                Ok(findings) => self.record(service, &ctx, &name, findings).await?,
                Err(e) => ReviewerOutcome {
                    name,
                    findings: Vec::new(),
                    thread_ids: Vec::new(),
                    vote: None,
                    error: Some(e.to_string()),
                },
            };
            outcomes.push(outcome);
        }
        Ok(PoolReport {
            change_id: *change_id,
            version: ctx.version,
            outcomes,
            truncated: ctx.truncated,
        })
    }

    async fn fan_out(&self, ctx: Arc<ReviewContext>) -> Vec<Result<Vec<Finding>>> {
        let per_reviewer = self.config.budget.timeout;
        let runs = self.reviewers.iter().map(|r| {
            let ctx = ctx.clone();
            async move {
                match tokio::time::timeout(per_reviewer, r.review(ctx)).await {
                    Ok(result) => result,
                    Err(_) => Err(Error::ReviewerTimeout(r.name().to_string())),
                }
            }
        });
        let all = futures::future::join_all(runs);
        match tokio::time::timeout(self.config.total_timeout, all).await {
            Ok(results) => results,
            Err(_) => self
                .reviewers
                .iter()
                .map(|r| Err(Error::ReviewerTimeout(r.name().to_string())))
                .collect(),
        }
    }

    async fn record(
        &self,
        service: &ReviewService,
        ctx: &ReviewContext,
        name: &str,
        findings: Vec<Finding>,
    ) -> Result<ReviewerOutcome> {
        let agent = Principal::agent(name);
        let mut thread_ids = Vec::new();
        for finding in &findings {
            if finding.confidence < self.config.min_confidence_to_post {
                continue;
            }
            let anchor = anchor_for(ctx.version, finding);
            let id = service
                .comment(
                    &ctx.change_id,
                    &agent,
                    anchor,
                    finding.body.clone(),
                    Some(finding.category),
                    Some(finding.confidence),
                    None,
                )
                .await?;
            thread_ids.push(id);
        }

        // A vote lands on the current version; if the change moved on
        // while the reviewer ran, the findings are stale and no vote is cast.
        let state = service.get(&ctx.change_id).await?;
        if state.current_version() != Some(ctx.version) {
            return Ok(ReviewerOutcome {
                name: name.to_string(),
                findings,
                thread_ids,
                vote: None,
                error: Some("version changed during review".into()),
            });
        }
        let blocking = findings.iter().filter(|f| self.config.blocks(f)).count();
        let (value, reason) = if blocking > 0 {
            (-1, format!("{blocking} blocking finding(s)"))
        } else {
            (1, format!("{} finding(s), none blocking", findings.len()))
        };
        match service
            .vote(&ctx.change_id, &agent, REVIEW_LABEL, value, Some(reason))
            .await
        {
            Ok(_) => Ok(ReviewerOutcome {
                name: name.to_string(),
                findings,
                thread_ids,
                vote: Some(value),
                error: None,
            }),
            // Voting can legitimately fail (status moved, label removed);
            // the findings are still posted, so report rather than abort.
            Err(e) => Ok(ReviewerOutcome {
                name: name.to_string(),
                findings,
                thread_ids,
                vote: None,
                error: Some(e.to_string()),
            }),
        }
    }
}

fn anchor_for(version: ChangeVersionHash, finding: &Finding) -> ThreadAnchor {
    match (&finding.path, finding.lines) {
        (Some(path), Some((start, end))) => ThreadAnchor::Lines {
            version,
            path: path.clone(),
            start,
            end,
            context_hash: String::new(),
        },
        (Some(path), None) => ThreadAnchor::File {
            version,
            path: path.clone(),
        },
        (None, _) => ThreadAnchor::Change,
    }
}
