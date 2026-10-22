//! Verification runs: listing, watching, and reading their logs and
//! artifacts by byte range from the org's execution space.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use rill_exec::remote::BlobDigest;
use rill_exec::runs::{Run, RunStore};
use rill_proto::rill::v1 as pb;
use rill_types::ObjectHash;
use tokio_stream::wrappers::ReceiverStream;

use super::convert;
use super::service::ChecksSvc;
use crate::services::common::{open, Opened};
use crate::services::Call;
use connectrpc::ConnectError;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// Default page of runs.
const DEFAULT_RUNS: usize = 20;
/// Most bytes one read returns.
const MAX_READ: u64 = 1 << 20;
/// A watch re-reads the run at least this often.
const KEEPALIVE: Duration = Duration::from_secs(30);

pub(super) type RunStream =
    Pin<Box<dyn Stream<Item = std::result::Result<pb::VerifyRun, ConnectError>> + Send>>;

fn runs_of(state: &AppState, opened: &Opened) -> RunStore {
    RunStore::new(
        opened.repo.repo.clone(),
        Duration::from_secs(state.config.verify.retention_days * 24 * 3600),
    )
}

async fn run(runs: &RunStore, id: &str) -> Result<Run> {
    runs.get(id)
        .await?
        .ok_or_else(|| Error::not_found(format!("run {id:?}")))
}

impl ChecksSvc {
    pub(super) async fn list_runs(
        &self,
        request: Call<pb::ListRunsRequest>,
    ) -> Result<pb::ListRunsResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        let version = match req.version.is_empty() {
            true => None,
            false => Some(ObjectHash::from_slice(&req.version)?),
        };
        let limit = match req.limit as usize {
            0 => DEFAULT_RUNS,
            n => n,
        };
        let headers = runs_of(&self.state, &opened)
            .list(version.as_ref(), limit)
            .await?;
        Ok(pb::ListRunsResponse {
            runs: headers.iter().map(|h| convert::run(h, &[])).collect(),
        })
    }

    pub(super) async fn get_run(&self, request: Call<pb::GetRunRequest>) -> Result<pb::VerifyRun> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let found = run(&runs_of(&self.state, &opened), &request.get_ref().run).await?;
        Ok(convert::run(&found.header, &found.steps))
    }

    pub(super) async fn watch_run(&self, request: Call<pb::GetRunRequest>) -> Result<RunStream> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let runs = runs_of(&self.state, &opened);
        let id = request.into_inner().run;
        run(&runs, &id).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let state: Arc<AppState> = self.state.clone();
        tokio::spawn(async move {
            loop {
                let seen = match runs.updates(&id).await {
                    Ok(seen) => seen,
                    Err(e) => {
                        let _ = tx.send(Err(ConnectError::from(Error::from(e)))).await;
                        return;
                    }
                };
                let current = match run(&runs, &id).await {
                    Ok(current) => current,
                    Err(e) => {
                        let _ = tx.send(Err(ConnectError::from(e))).await;
                        return;
                    }
                };
                let done = current.header.end.is_some();
                let message = convert::run(&current.header, &current.steps);
                if tx.send(Ok(message)).await.is_err() || done {
                    return;
                }
                let waited = tokio::select! {
                    _ = state.lifecycle.stop.cancelled() => return,
                    waited = runs.wait(&id, seen, KEEPALIVE) => waited,
                };
                if let Err(e) = waited {
                    let _ = tx.send(Err(ConnectError::from(Error::from(e)))).await;
                    return;
                }
            }
        });
        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    pub(super) async fn read_log(
        &self,
        request: Call<pb::ReadLogRequest>,
    ) -> Result<pb::ReadBytesResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        let found = run(&runs_of(&self.state, &opened), &req.run).await?;
        let step = found
            .step(req.step)
            .ok_or_else(|| Error::not_found(format!("step {} of run {}", req.step, req.run)))?;
        let digest = match req.stream.as_known() {
            Some(pb::LogStream::Stderr) => step.stderr,
            _ => step.stdout,
        };
        let Some(digest) = digest else {
            return Ok(pb::ReadBytesResponse::default());
        };
        self.read_range(&opened, digest.into(), req.offset, req.limit)
            .await
    }

    pub(super) async fn read_artifact(
        &self,
        request: Call<pb::ReadArtifactRequest>,
    ) -> Result<pb::ReadBytesResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        let found = run(&runs_of(&self.state, &opened), &req.run).await?;
        let step = found
            .step(req.step)
            .ok_or_else(|| Error::not_found(format!("step {} of run {}", req.step, req.run)))?;
        let digest = step
            .outputs
            .iter()
            .find(|(path, _)| *path == req.path)
            .map(|(_, d)| *d)
            .ok_or_else(|| Error::not_found(format!("artifact {:?}", req.path)))?;
        self.read_range(&opened, digest.into(), req.offset, req.limit)
            .await
    }

    /// `limit` bytes of `digest` from `offset`, a negative offset counting
    /// from the end.
    async fn read_range(
        &self,
        opened: &Opened,
        digest: BlobDigest,
        offset: i64,
        limit: u32,
    ) -> Result<pb::ReadBytesResponse> {
        let size = digest.size;
        let start = match offset {
            o if o < 0 => size.saturating_sub(o.unsigned_abs()),
            o => (o as u64).min(size),
        };
        let len = match limit {
            0 => MAX_READ,
            n => u64::from(n).min(MAX_READ),
        }
        .min(size - start);
        let space = self.state.exec.space(&opened.repo.config.org);
        let data = space
            .read_range(&digest, start, len)
            .await?
            .ok_or_else(|| Error::not_found(format!("content {digest} is no longer stored")))?;
        Ok(pb::ReadBytesResponse {
            data,
            offset: start,
            size,
        })
    }
}
