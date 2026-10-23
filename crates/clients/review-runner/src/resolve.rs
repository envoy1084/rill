//! The resolve loop: lease a conflicted version, read its terms, resolve
//! them on the backend, store the resolution as the agent, report.

use bytes::Bytes;
use rill_sdk::{RepoClient, Resolution, ResolveJob};
use rill_types::objects::change::FileChange;
use rill_types::{FileKind, Provenance, RepoPathBuf};

use crate::backend::{Backend, ConflictFile, ResolveOutcome, ResolveTask};
use crate::error::{Error, Result};
use crate::review::{with_renewal, Tally};

async fn text(
    repo: &RepoClient,
    state: Option<&rill_types::objects::change::FileState>,
) -> Result<Option<String>> {
    let Some(Some((content, _))) = state else {
        return Ok(None);
    };
    let bytes = repo.get_blob(content).await?;
    Ok(String::from_utf8(bytes.to_vec()).ok())
}

/// The conflicted files of `job`'s version, with each term's text, as
/// the version records them.
pub async fn gather(repo: &RepoClient, job: &ResolveJob) -> Result<ResolveTask> {
    let cv = repo.change_version(job.version).await?;
    let mut files = Vec::new();
    for (path, change) in &cv.file_changes {
        let FileChange::Conflict(terms) = change else {
            continue;
        };
        if !job.paths.iter().any(|p| p == path.as_str()) {
            continue;
        }
        let mut sides = Vec::new();
        for side in terms.adds() {
            sides.push(text(repo, Some(side)).await?);
        }
        files.push(ConflictFile {
            path: path.to_string(),
            base: text(repo, terms.removes().next()).await?,
            sides,
        });
    }
    Ok(ResolveTask {
        prompt: job.prompt.clone(),
        files,
    })
}

/// Resolve jobs until none is waiting (`once`) or forever.
pub async fn run(
    repo: &RepoClient,
    backend: &dyn Backend,
    wait_ms: u32,
    once: bool,
) -> Result<Tally> {
    let reviewers = repo.reviewers();
    let mut tally = Tally::default();
    loop {
        let Some((lease, job)) = reviewers.lease_resolve(wait_ms).await? else {
            if once {
                return Ok(tally);
            }
            continue;
        };
        tracing::info!(change = %job.change, version = %job.version, "resolving");
        let (lease, outcome) = with_renewal(&reviewers, lease, async {
            let task = gather(repo, &job).await?;
            backend.resolve(&task).await
        })
        .await;
        let reported = match outcome {
            Ok(ResolveOutcome::Resolved {
                files,
                rationale,
                model,
            }) => {
                let resolutions = files
                    .into_iter()
                    .map(|(path, content)| {
                        let path =
                            RepoPathBuf::new(&path).map_err(|e| Error::Config(e.to_string()))?;
                        Ok(match content {
                            Some(text) => Resolution::Set {
                                path,
                                bytes: Bytes::from(text),
                                kind: FileKind::Regular,
                            },
                            None => Resolution::Delete { path },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let provenance = Provenance {
                    tool: Some("rill-review-runner".into()),
                    model: Some(model),
                    resolved_by: Some("agent".into()),
                    ..Provenance::default()
                };
                let stored = repo.resolve(job.version, resolutions, provenance).await?;
                reviewers
                    .complete_resolve(&lease, Some(stored.hash), &rationale)
                    .await
            }
            Ok(ResolveOutcome::Declined(why)) => {
                reviewers.complete_resolve(&lease, None, &why).await
            }
            Err(e) => {
                tracing::warn!(change = %job.change, error = %e, "the backend failed; the job will be retried");
                reviewers.fail(&lease, &e.to_string(), true).await?;
                tally.failed += 1;
                continue;
            }
        };
        match reported {
            Ok(run) => {
                tracing::info!(change = %job.change, detail = %run.detail, "resolve reported");
                tally.done += 1;
            }
            Err(e) => {
                tracing::warn!(change = %job.change, error = %e, "the server refused the resolution");
                tally.failed += 1;
            }
        }
    }
}
