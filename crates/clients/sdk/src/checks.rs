//! Checks: verification runs with their logs and artifacts, test history,
//! flakes and quarantines, on one repository.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::ChangeVersionHash;

use crate::client::{retry_idempotent, Client};
use crate::error::Result;
use crate::paging;
use crate::repo::map_stream;
use crate::stream::EventStream;
use crate::types::Page;

/// Checks on one repository.
#[derive(Clone, Debug)]
pub struct ChecksClient {
    client: Client,
    /// The repository's full resource name.
    repo: String,
}

impl ChecksClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        ChecksClient { client, repo }
    }

    fn svc(&self) -> crate::client::ChecksSvc {
        self.client.checks_svc()
    }

    fn run_name(&self, id: &str) -> String {
        format!("{}/runs/{id}", self.repo)
    }

    /// One page of runs, newest first, all or of one version.
    pub async fn runs(
        &self,
        version: Option<&ChangeVersionHash>,
        page_size: u32,
        page_token: Option<&str>,
    ) -> Result<Page<pb::VerifyRun>> {
        let request = pb::ListRunsRequest {
            parent: self.repo.clone(),
            page_size: page_size as i32,
            page_token: page_token.unwrap_or_default().to_string(),
            version: version
                .map(|v| crate::names::version(&self.repo, v))
                .unwrap_or_default(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_runs(request).await }
        })
        .await?
        .into_owned();
        Ok(Page::new(resp.runs, token(resp.next_page_token)))
    }

    /// A run with its steps.
    pub async fn run(&self, id: &str) -> Result<pb::VerifyRun> {
        let name = self.run_name(id);
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetRunRequest { name: name.clone() };
            async move { svc.get_run(request).await }
        })
        .await?
        .into_owned();
        Ok(required("get_run.run", resp.run)?)
    }

    /// The run now and after every change, until it ends.
    pub async fn watch_run(&self, id: &str) -> Result<EventStream<pb::VerifyRun>> {
        let stream = self
            .svc()
            .watch_run(pb::WatchRunRequest {
                name: self.run_name(id),
            })
            .await?;
        Ok(map_stream(stream, |r: pb::WatchRunResponse| {
            Ok(required("watch_run.run", r.run)?)
        }))
    }

    /// Up to `limit` bytes (0: the server's most) of a step's standard
    /// output or error from `offset`, a negative offset counting from the
    /// end; with where they start and the whole size.
    pub async fn read_log(
        &self,
        run: &str,
        step: u32,
        stderr: bool,
        offset: i64,
        limit: u32,
    ) -> Result<pb::ReadLogResponse> {
        let request = pb::ReadLogRequest {
            run: self.run_name(run),
            step,
            stream: match stderr {
                true => pb::LogStream::Stderr,
                false => pb::LogStream::Stdout,
            }
            .into(),
            offset,
            limit,
        };
        Ok(retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.read_log(request).await }
        })
        .await?
        .into_owned())
    }

    /// Up to `limit` bytes of a file a step produced, from `offset`.
    pub async fn read_artifact(
        &self,
        run: &str,
        step: u32,
        path: &str,
        offset: i64,
        limit: u32,
    ) -> Result<pb::ReadArtifactResponse> {
        let request = pb::ReadArtifactRequest {
            run: self.run_name(run),
            step,
            path: path.to_string(),
            offset,
            limit,
        };
        Ok(retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.read_artifact(request).await }
        })
        .await?
        .into_owned())
    }

    /// The latest `max_results` runs of a step or test, with its flake
    /// stats.
    pub async fn test_history(
        &self,
        target: &str,
        max_results: u32,
    ) -> Result<pb::GetTestHistoryResponse> {
        let request = pb::GetTestHistoryRequest {
            parent: self.repo.clone(),
            target: target.to_string(),
            max_results: max_results as i32,
        };
        Ok(retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.get_test_history(request).await }
        })
        .await?
        .into_owned())
    }

    /// One page of targets flakier than `min_score` (the server's default
    /// when `None`), flakiest first.
    pub async fn flaky_tests(
        &self,
        min_score: Option<f64>,
        page_size: u32,
        page_token: Option<&str>,
    ) -> Result<Page<pb::FlakyTest>> {
        let request = pb::ListFlakyTestsRequest {
            parent: self.repo.clone(),
            page_size: page_size as i32,
            page_token: page_token.unwrap_or_default().to_string(),
            min_score,
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.list_flaky_tests(request).await }
        })
        .await?
        .into_owned();
        Ok(Page::new(resp.tests, token(resp.next_page_token)))
    }

    /// Steps a hermeticity probe caught, running uncached until the flag
    /// expires.
    pub async fn nonhermetic_steps(&self) -> Result<Vec<pb::NonhermeticStep>> {
        paging::all(|page_token| async move {
            let request = pb::ListNonhermeticStepsRequest {
                parent: self.repo.clone(),
                page_size: paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_nonhermetic_steps(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.steps, resp.next_page_token))
        })
        .await
    }

    /// Every quarantined target.
    pub async fn quarantines(&self) -> Result<Vec<pb::Quarantine>> {
        paging::all(|page_token| async move {
            let request = pb::ListQuarantinesRequest {
                parent: self.repo.clone(),
                page_size: paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_quarantines(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.quarantines, resp.next_page_token))
        })
        .await
    }

    /// Quarantine a step (`step:<name>`) or test (`test:<id>`); owners.
    pub async fn quarantine(&self, target: &str, reason: &str) -> Result<pb::Quarantine> {
        let request = pb::CreateQuarantineRequest {
            request_id: crate::names::request_id(&rill_types::OperationId::new()),
            parent: self.repo.clone(),
            quarantine: pb::Quarantine {
                target: target.to_string(),
                reason: reason.to_string(),
                ..Default::default()
            }
            .into(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_quarantine(request).await }
        })
        .await?
        .into_owned();
        Ok(required("create_quarantine.quarantine", resp.quarantine)?)
    }

    /// Lift a quarantine; owners.
    pub async fn release_quarantine(&self, target: &str) -> Result<()> {
        let request = pb::DeleteQuarantineRequest {
            request_id: crate::names::request_id(&rill_types::OperationId::new()),
            name: format!("{}/quarantines/{target}", self.repo),
        };
        retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_quarantine(request).await }
        })
        .await?;
        Ok(())
    }
}

/// A page token as the SDK returns it: `None` after the last page.
pub(crate) fn token(next: String) -> Option<String> {
    Some(next).filter(|t| !t.is_empty())
}
