//! Verification runs: listing, watching, and reading their logs and
//! artifacts by byte range from the org's execution space.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use rill_exec::remote::BlobDigest;
use rill_exec::runs::{Run, RunStore};
use rill_proto::rill::v1 as pb;
use tokio_stream::wrappers::ReceiverStream;

use super::convert;
use super::service::ChecksSvc;
use crate::listing::{Page, Position};
use crate::services::common::{open_repo, Opened};
use crate::services::{names, Call};
use connectrpc::ConnectError;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// Most bytes one read returns.
const MAX_READ: u64 = 1 << 20;
/// A watch re-reads the run at least this often.
const KEEPALIVE: Duration = Duration::from_secs(30);

pub(super) type RunStream =
    Pin<Box<dyn Stream<Item = std::result::Result<pb::WatchRunResponse, ConnectError>> + Send>>;

fn runs_of(state: &AppState, opened: &Opened) -> RunStore {
    RunStore::new(
        opened.repo.repo.clone(),
        Duration::from_secs(state.config.verify.retention_days * 24 * 3600),
    )
}

fn run_proto(
    opened: &Opened,
    header: &rill_exec::runs::RunHeader,
    steps: &[rill_exec::runs::IndexedStep],
) -> pb::VerifyRun {
    pb::VerifyRun {
        name: names::run(&opened.repo, &header.id),
        ..convert::run(header, steps)
    }
}

async fn run(runs: &RunStore, id: &str) -> Result<Run> {
    runs.get(id)
        .await?
        .ok_or_else(|| Error::not_found(format!("run {id:?}")))
}

impl ChecksSvc {
    pub(super) async fn list_runs_impl(
        &self,
        request: Call<pb::ListRunsRequest>,
    ) -> Result<pb::ListRunsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read_any().await?;
        let version = match req.version.is_empty() {
            true => None,
            false => Some(names::version_in(&opened.repo, &req.version)?),
        };
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.version])?;
        let skip = match &page.start {
            Position::Start => 0,
            Position::Offset(n) => *n as usize,
            Position::After(_) => {
                return Err(Error::InvalidPageToken("not a token for this list".into()))
            }
        };
        let mut headers = runs_of(&self.state, &opened)
            .list(version.as_ref(), skip + page.size + 1)
            .await?;
        let more = headers.len() > skip + page.size;
        headers.truncate(skip + page.size);
        let runs = headers
            .iter()
            .skip(skip)
            .map(|h| run_proto(&opened, h, &[]))
            .collect();
        Ok(pb::ListRunsResponse {
            runs,
            next_page_token: match more {
                true => page.token(Position::Offset((skip + page.size) as u64)),
                false => String::new(),
            },
        })
    }

    pub(super) async fn get_run_impl(
        &self,
        request: Call<pb::GetRunRequest>,
    ) -> Result<pb::GetRunResponse> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read_any().await?;
        let id = names::run_in(&opened.repo, name)?;
        let found = run(&runs_of(&self.state, &opened), &id).await?;
        Ok(pb::GetRunResponse {
            run: run_proto(&opened, &found.header, &found.steps).into(),
        })
    }

    pub(super) async fn watch_run_impl(
        &self,
        request: Call<pb::WatchRunRequest>,
    ) -> Result<RunStream> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read_any().await?;
        let runs = runs_of(&self.state, &opened);
        let id = names::run_in(&opened.repo, name)?;
        let run_name = names::run(&opened.repo, &id);
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
                let message = pb::WatchRunResponse {
                    run: pb::VerifyRun {
                        name: run_name.clone(),
                        ..convert::run(&current.header, &current.steps)
                    }
                    .into(),
                };
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

    pub(super) async fn read_log_impl(
        &self,
        request: Call<pb::ReadLogRequest>,
    ) -> Result<pb::ReadLogResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().run).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        let id = names::run_in(&opened.repo, &req.run)?;
        let found = run(&runs_of(&self.state, &opened), &id).await?;
        let step = found
            .step(req.step)
            .ok_or_else(|| Error::not_found(format!("step {} of run {}", req.step, req.run)))?;
        let digest = match req.stream.as_known() {
            Some(pb::LogStream::Stderr) => step.stderr,
            _ => step.stdout,
        };
        let Some(digest) = digest else {
            return Ok(pb::ReadLogResponse::default());
        };
        let (data, offset, size) = self
            .read_range(&opened, digest.into(), req.offset, req.limit)
            .await?;
        Ok(pb::ReadLogResponse { data, offset, size })
    }

    pub(super) async fn read_artifact_impl(
        &self,
        request: Call<pb::ReadArtifactRequest>,
    ) -> Result<pb::ReadArtifactResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().run).await?;
        opened.permit().read_any().await?;
        let req = request.into_inner();
        let id = names::run_in(&opened.repo, &req.run)?;
        let found = run(&runs_of(&self.state, &opened), &id).await?;
        let step = found
            .step(req.step)
            .ok_or_else(|| Error::not_found(format!("step {} of run {}", req.step, req.run)))?;
        let digest = step
            .outputs
            .iter()
            .find(|(path, _)| *path == req.path)
            .map(|(_, d)| *d)
            .ok_or_else(|| Error::not_found(format!("artifact {:?}", req.path)))?;
        let (data, offset, size) = self
            .read_range(&opened, digest.into(), req.offset, req.limit)
            .await?;
        Ok(pb::ReadArtifactResponse { data, offset, size })
    }

    /// `limit` bytes of `digest` from `offset`, a negative offset counting
    /// from the end, with where they start and the whole size.
    async fn read_range(
        &self,
        opened: &Opened,
        digest: BlobDigest,
        offset: i64,
        limit: u32,
    ) -> Result<(bytes::Bytes, u64, u64)> {
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
        Ok((data, start, size))
    }
}
