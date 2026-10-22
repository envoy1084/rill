//! Pre-land and post-land verification through remote execution.

use std::sync::Arc;

use async_trait::async_trait;
use rill_exec::history::{Quarantine, TestHistory};
use rill_exec::{PlanRunner, Target};
use rill_jobs::Priority;
use rill_lander::{Failure, VerificationOutcome, Verifier, VerifyRequest, VerifyStage};
use rill_types::{Intent, IntentKind, Principal, Timestamp};

use crate::config::FlakeConfig;

/// Runs a tree's `.rill/verify.toml` through the scheduler and reports
/// the outcome to the lander. A failure only of flaky targets is run
/// again before it counts, and a target that keeps flipping is
/// quarantined.
pub struct ExecVerifier {
    runner: Arc<PlanRunner>,
    history: TestHistory,
    flakes: FlakeConfig,
}

/// Who quarantines targets on its own.
fn verifier_principal() -> Principal {
    Principal::service("verifier")
}

impl ExecVerifier {
    /// A verifier running plans with `runner` and recording what ran in
    /// `history`.
    pub fn new(runner: Arc<PlanRunner>, history: TestHistory, flakes: FlakeConfig) -> Self {
        ExecVerifier {
            runner,
            history,
            flakes,
        }
    }

    /// Run the plan once and record what ran.
    async fn run_once(
        &self,
        target: &Target,
    ) -> rill_lander::Result<Result<rill_exec::VerificationOutcome, String>> {
        let outcome = match self.runner.verify(target).await {
            Ok(outcome) => outcome,
            // A plan that cannot run as written fails the change that
            // carries it rather than the verifier.
            Err(rill_exec::Error::Config { path, reason }) => {
                return Ok(Err(format!("invalid {path}: {reason}")))
            }
            Err(e) => return Err(rill_lander::Error::Verifier(e.to_string())),
        };
        for run in &outcome.test_runs(target.version, target.trunk, Timestamp::now()) {
            if let Err(e) = self.history.record(run).await {
                tracing::warn!(target = %run.target, error = %e, "recording a test run");
            }
        }
        Ok(Ok(outcome))
    }

    /// What failed in `outcome`, with flake scores.
    async fn failures(&self, outcome: &rill_exec::VerificationOutcome) -> Vec<Failure> {
        let now = Timestamp::now().millis();
        let mut out = Vec::new();
        for step in outcome.steps.iter().filter(|s| !s.status.is_ok()) {
            let mut targets = vec![step.target()];
            targets.extend(step.failed_tests());
            for target in targets {
                let flake = match self.history.stats(&target).await {
                    Ok(Some(stats)) => stats.score(now),
                    _ => 0.0,
                };
                out.push(Failure {
                    target,
                    step: step.step.clone(),
                    flake_permille: (flake * 1000.0).round() as u32,
                });
            }
        }
        out
    }

    /// Quarantine every failure that has flipped often enough, opening an
    /// intent to fix it.
    async fn quarantine_chronic(&self, failures: &[Failure]) {
        let now = Timestamp::now().millis();
        for failure in failures {
            let Ok(Some(stats)) = self.history.stats(&failure.target).await else {
                continue;
            };
            let score = stats.score(now);
            if score * 1000.0 <= f64::from(self.flakes.quarantine_above_permille)
                || stats.flips < f64::from(self.flakes.quarantine_flips)
            {
                continue;
            }
            let mut intent = Intent::new(
                format!("Fix flaky {}", failure.target),
                IntentKind::System,
                verifier_principal(),
            );
            intent.body = format!(
                "{} was quarantined: flake score {score:.2} after {:.0} flips. Its \\
                 failures no longer block landings or make trunk red until the \\
                 quarantine is lifted.",
                failure.target, stats.flips
            );
            let quarantine = Quarantine {
                target: failure.target.clone(),
                since: Timestamp::now(),
                by: verifier_principal(),
                reason: format!("flake score {score:.2}, {:.0} flips", stats.flips),
                intent: Some(intent.id),
            };
            match self.history.quarantine(&quarantine).await {
                Ok(true) => {
                    tracing::warn!(target = %failure.target, score, "quarantined a flaky target");
                    if let Err(e) = self
                        .history
                        .repo()
                        .put_intent(&intent, &verifier_principal())
                        .await
                    {
                        tracing::warn!(error = %e, "opening the quarantine intent");
                    }
                }
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "quarantining a flaky target"),
            }
        }
    }
}

#[async_trait]
impl Verifier for ExecVerifier {
    async fn verify(&self, request: VerifyRequest) -> rill_lander::Result<VerificationOutcome> {
        let target = Target {
            root: request.root,
            version: request.version,
            base: request.base,
            risk: request.risk,
            priority: match request.stage {
                VerifyStage::PreLand => Priority::High,
                VerifyStage::PostLand => Priority::Normal,
            },
            trunk: request.stage == VerifyStage::PostLand,
        };
        let mut outcome = match self.run_once(&target).await? {
            Ok(outcome) => outcome,
            Err(invalid) => return Ok(VerificationOutcome::red(invalid)),
        };
        let mut failures = self.failures(&outcome).await;
        let threshold = self.flakes.rerun_above_permille;
        let mut reruns = 0;
        while !outcome.green
            && reruns < self.flakes.reruns
            && !failures.is_empty()
            && failures.iter().all(|f| f.flake_permille > threshold)
        {
            reruns += 1;
            tracing::info!(version = %request.version, reruns, "re-running flaky failures");
            outcome = match self.run_once(&target).await? {
                Ok(outcome) => outcome,
                Err(invalid) => return Ok(VerificationOutcome::red(invalid)),
            };
            if !outcome.green {
                failures = self.failures(&outcome).await;
            }
        }
        if !failures.is_empty() {
            self.quarantine_chronic(&failures).await;
        }
        let mut details = outcome.summary();
        if let Some(run) = &outcome.run {
            details.push_str(&format!(" (run {run})"));
        }
        if let Some(excerpt) = &outcome.excerpt {
            details.push('\n');
            details.push_str(excerpt);
        }
        tracing::info!(version = %request.version, green = outcome.green, reruns, summary = %outcome.summary(), "verification");
        Ok(if outcome.green {
            VerificationOutcome {
                flaky: reruns > 0,
                ..VerificationOutcome::green(details)
            }
        } else {
            VerificationOutcome::red(details).with_failures(failures)
        })
    }

    async fn could_affect(
        &self,
        root: rill_types::TreeHash,
        base: Option<rill_types::TreeHash>,
        steps: &std::collections::BTreeSet<String>,
    ) -> rill_lander::Result<bool> {
        match self.runner.could_affect(root, base, steps).await {
            Ok(affects) => Ok(affects),
            Err(e) => {
                tracing::warn!(error = %e, "could not tell what a change affects");
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use rill_core::tree::{Edit, TreeBuilder};
    use rill_core::{store_content, TreeOps};
    use rill_exec::history::{TestOutcome, TestRun};
    use rill_exec::remote::{Scheduler, SchedulerConfig};
    use rill_exec::{LocalSandbox, LocalWorker, PLAN_PATH};
    use rill_store::Stores;
    use rill_types::{FileKind, ObjectHash, ObjectKind, RepoPathBuf, TreeHash};

    use super::*;

    struct Setup {
        verifier: ExecVerifier,
        history: TestHistory,
        root: TreeHash,
        _marker: tempfile::TempDir,
        worker: tokio::task::JoinHandle<()>,
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            self.worker.abort();
        }
    }

    /// A repository whose one step fails the first time it runs and passes
    /// after, and whose history already flipped `flips` times.
    async fn setup(flips: usize) -> Setup {
        let stores = Stores::memory();
        let repo = stores.repo(stores.create_repo("app", "").await.unwrap().id);
        let trees = TreeOps::new(repo.objects().clone(), 64);
        let scheduler = Arc::new(Scheduler::new(
            stores.clone(),
            SchedulerConfig {
                idle_wait: Duration::from_millis(200),
                ..SchedulerConfig::default()
            },
        ));
        let worker = LocalWorker::new(
            scheduler.clone(),
            Arc::new(LocalSandbox::new(false)),
            "w",
            &Default::default(),
        );
        let worker = tokio::spawn(worker.run(std::future::pending()));
        let marker = tempfile::tempdir().unwrap();
        let seen = marker.path().join("seen");
        let plan = format!(
            "[[step]]\nname = \"unit\"\ncommand = [\"sh\", \"-c\", \
             \"test -e {0} || {{ touch {0}; exit 1; }}\"]\n",
            seen.display()
        );
        let mut builder = TreeBuilder::new();
        let content = store_content(repo.objects().as_ref(), Bytes::from(plan))
            .await
            .unwrap();
        builder.edit(
            &RepoPathBuf::new(PLAN_PATH).unwrap(),
            Edit::Set(content, FileKind::Regular),
        );
        let root = builder.build(&trees, None).await.unwrap();
        let history = TestHistory::new(repo.clone());
        let action = Some(ObjectHash::digest(ObjectKind::Derived, b"earlier"));
        for i in 0..=flips {
            let outcome = match i % 2 {
                0 => TestOutcome::Passed,
                _ => TestOutcome::Failed,
            };
            history
                .record(&TestRun {
                    target: "step:unit".into(),
                    outcome,
                    retried: false,
                    duration_ms: None,
                    action,
                    worker: None,
                    version: ObjectHash::ZERO,
                    trunk: true,
                    // A second apart, so the history orders them.
                    at: Timestamp::from_millis(
                        Timestamp::now().millis() - 1000 * (flips - i) as i64,
                    ),
                })
                .await
                .unwrap();
        }
        let runner = PlanRunner::new(
            scheduler,
            rill_store::DEFAULT_ORG,
            "app",
            trees,
            Default::default(),
        )
        .with_env(crate::config::VerifyConfig::default().env);
        Setup {
            verifier: ExecVerifier::new(Arc::new(runner), history.clone(), FlakeConfig::default()),
            history,
            root,
            _marker: marker,
            worker,
        }
    }

    fn request(root: TreeHash) -> VerifyRequest {
        VerifyRequest {
            root,
            version: ObjectHash::ZERO,
            base: None,
            risk: None,
            stage: VerifyStage::PostLand,
        }
    }

    #[tokio::test]
    async fn a_failure_of_a_flaky_target_is_run_again_and_passes_as_a_flake() {
        let s = setup(2).await;
        let outcome = s.verifier.verify(request(s.root)).await.unwrap();
        assert!(outcome.green && outcome.flaky, "{}", outcome.details);
        assert!(
            s.history.quarantined().await.unwrap().is_empty(),
            "two flips are not chronic"
        );
    }

    #[tokio::test]
    async fn a_failure_of_a_stable_target_counts_at_once() {
        let s = setup(0).await;
        let outcome = s.verifier.verify(request(s.root)).await.unwrap();
        assert!(!outcome.green);
        assert_eq!(outcome.failures[0].target, "step:unit");
    }

    #[tokio::test]
    async fn a_target_that_keeps_flipping_is_quarantined_with_an_intent() {
        let s = setup(7).await;
        s.verifier.verify(request(s.root)).await.unwrap();
        let quarantined = s.history.quarantined().await.unwrap();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].target, "step:unit");
        let intent = s
            .history
            .repo()
            .get_intent(&quarantined[0].intent.unwrap())
            .await
            .unwrap()
            .unwrap();
        assert!(intent.title.contains("step:unit"));
    }
}
