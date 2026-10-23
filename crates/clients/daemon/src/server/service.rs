//! The `DaemonService` over a [`Daemon`].

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use futures::{StreamExt, TryStreamExt};
use rill_sdk::Root;
use rill_types::{IntentId, Provenance, RepoPathBuf};

use super::convert::{change_to_proto, mode_from_proto, mount_to_proto, to_connect};
use crate::daemon::{Daemon, MountSpec};
use crate::error::{Error, Result};
use crate::mount::CommitOptions;
use crate::proto as pb;

type Reply<T> = std::result::Result<Response<T>, ConnectError>;

/// The daemon's service.
#[derive(Clone)]
pub struct DaemonSvc {
    daemon: Arc<Daemon>,
}

impl DaemonSvc {
    /// A service over `daemon`.
    pub fn new(daemon: Arc<Daemon>) -> Self {
        DaemonSvc { daemon }
    }
}

fn reply<T>(result: Result<T>) -> Reply<T> {
    result.map(Response::new).map_err(to_connect)
}

fn paths(raw: &[String]) -> Result<Vec<RepoPathBuf>> {
    raw.iter()
        .map(|p| RepoPathBuf::new(p).map_err(Into::into))
        .collect()
}

fn revision(raw: &str) -> Result<Option<Root>> {
    if raw.is_empty() {
        return Ok(None);
    }
    Root::from_str(raw)
        .map(Some)
        .map_err(|e| Error::Invalid(format!("revision {raw:?}: {e}")))
}

fn opt(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl pb::DaemonService for DaemonSvc {
    async fn get_daemon(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, pb::GetDaemonRequest>,
    ) -> Reply<pb::GetDaemonResponse> {
        let s = self.daemon.cache_stats();
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3;
        let tier = |t: rill_sdk::read_stats::TierStats| pb::TierStats {
            reads: t.reads,
            p50_ms: ms(t.p50),
            p99_ms: ms(t.p99),
        };
        let reads = self.daemon.read_stats();
        let (local, p50, p99) = self.daemon.local_reads();
        let daemon = self.daemon.clone();
        let warnings = tokio::task::spawn_blocking(move || crate::doctor::check(&daemon))
            .await
            .unwrap_or_default();
        let dir = self.daemon.config().dir.clone();
        let free = crate::config::disk_space(&dir.cache()).map_or(0, |(_, f)| f);
        reply(Ok(pb::GetDaemonResponse {
            version: env!("CARGO_PKG_VERSION").into(),
            pid: std::process::id(),
            cache: pb::CacheStats {
                objects: s.objects,
                bytes: s.bytes,
                budget_bytes: s.budget,
                hits: s.hits,
                misses: s.misses,
                evictions: s.evictions,
            }
            .into(),
            mounts: self.daemon.mounts().len() as u32,
            local: pb::TierStats {
                reads: local,
                p50_ms: ms(p50),
                p99_ms: ms(p99),
            }
            .into(),
            regional: tier(reads.regional()).into(),
            origin: tier(reads.origin()).into(),
            regional_fallbacks: reads.regional_fallbacks,
            cache_free_bytes: free,
            warnings: warnings
                .into_iter()
                .map(|w| pb::Warning {
                    subject: w.subject,
                    message: w.message,
                    fix: w.fix,
                })
                .collect(),
            dir: dir.root().display().to_string(),
        }))
    }

    async fn create_mount(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateMountRequest>,
    ) -> Reply<pb::CreateMountResponse> {
        let r = request.to_owned_message();
        reply(
            async {
                let spec = MountSpec {
                    path: PathBuf::from(&r.path),
                    endpoint: r.endpoint.clone(),
                    token: opt(r.token.clone()),
                    repo: r.repo.clone(),
                    revision: revision(&r.revision)?,
                    sparse: paths(&r.sparse)?,
                    mode: mode_from_proto(r.mode),
                    no_workspace: r.no_workspace,
                };
                let (mount, files) = self.daemon.create_mount(spec).await?;
                Ok(pb::CreateMountResponse {
                    mount: mount_to_proto(&mount).into(),
                    files,
                })
            }
            .await,
        )
    }

    async fn get_mount(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetMountRequest>,
    ) -> Reply<pb::GetMountResponse> {
        let r = request.to_owned_message();
        reply((|| {
            let mount = if r.name.is_empty() {
                self.daemon.find(std::path::Path::new(&r.path))?
            } else {
                self.daemon.mount(&r.name)?
            };
            self.daemon.offer_token(&mount.record().endpoint, &r.token);
            Ok(pb::GetMountResponse {
                mount: mount_to_proto(&mount).into(),
            })
        })())
    }

    async fn list_mounts(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, pb::ListMountsRequest>,
    ) -> Reply<pb::ListMountsResponse> {
        reply(Ok(pb::ListMountsResponse {
            mounts: self
                .daemon
                .mounts()
                .iter()
                .map(|m| mount_to_proto(m))
                .collect(),
        }))
    }

    async fn delete_mount(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::DeleteMountRequest>,
    ) -> Reply<pb::DeleteMountResponse> {
        let name = request.name.to_string();
        reply(
            self.daemon
                .delete_mount(&name)
                .await
                .map(|()| pb::DeleteMountResponse::default()),
        )
    }

    async fn get_status(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetStatusRequest>,
    ) -> Reply<pb::GetStatusResponse> {
        let r = request.to_owned_message();
        reply(
            async {
                let mount = self.daemon.mount(&r.name)?;
                let changes = mount.status(&paths(&r.prefixes)?).await?;
                Ok(pb::GetStatusResponse {
                    mount: mount_to_proto(&mount).into(),
                    changes: changes.iter().map(change_to_proto).collect(),
                })
            }
            .await,
        )
    }

    async fn sync_mount(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::SyncMountRequest>,
    ) -> Reply<pb::SyncMountResponse> {
        let name = request.name.to_string();
        reply(
            async {
                let mount = self.daemon.mount(&name)?;
                mount.settle().await?;
                let pushed = mount.push().await?;
                *mount.sync_error.lock() = None;
                Ok(pb::SyncMountResponse {
                    mount: mount_to_proto(&mount).into(),
                    pushed,
                })
            }
            .await,
        )
    }

    async fn commit_mount(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::CommitMountRequest>,
    ) -> Reply<pb::CommitMountResponse> {
        let r = request.to_owned_message();
        reply(
            async {
                let mount = self.daemon.mount(&r.name)?;
                let intent = match r.intent.rsplit('/').next().filter(|s| !s.is_empty()) {
                    Some(id) => Some(
                        IntentId::from_str(id)
                            .map_err(|e| Error::Invalid(format!("intent {id:?}: {e}")))?,
                    ),
                    None => None,
                };
                let opts = CommitOptions {
                    message: (!r.keep_message).then(|| r.message.clone()),
                    intent,
                    new_change: r.new_change,
                    provenance: Provenance {
                        tool: opt(r.tool.clone()),
                        model: opt(r.model.clone()),
                        session: opt(r.session.clone()),
                        ..Provenance::default()
                    },
                };
                let done = mount.commit(opts).await?;
                Ok(pb::CommitMountResponse {
                    mount: mount_to_proto(&mount).into(),
                    version: done.result.hash.to_hex(),
                    change: done.result.version.change_id.to_string(),
                    stacked_on: done.stacked_on.map(|c| c.to_string()).unwrap_or_default(),
                    message: done.result.version.message.clone(),
                })
            }
            .await,
        )
    }

    async fn update_mount(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::UpdateMountRequest>,
    ) -> Reply<pb::UpdateMountResponse> {
        let r = request.to_owned_message();
        reply(
            async {
                let mount = self.daemon.mount(&r.name)?;
                let onto = revision(&r.revision)?.unwrap_or_else(Root::trunk);
                let report = mount.update(onto).await?;
                let strs = |v: &[RepoPathBuf]| v.iter().map(|p| p.as_str().to_string()).collect();
                Ok(pb::UpdateMountResponse {
                    mount: mount_to_proto(&mount).into(),
                    updated: strs(&report.updated),
                    removed: strs(&report.removed),
                    merged: strs(&report.merged),
                    conflicts: strs(&report.conflicts),
                })
            }
            .await,
        )
    }

    async fn resolve_paths(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolvePathsRequest>,
    ) -> Reply<pb::ResolvePathsResponse> {
        let r = request.to_owned_message();
        reply(
            async {
                let mount = self.daemon.mount(&r.name)?;
                mount.resolve(&paths(&r.paths)?, r.force).await?;
                Ok(pb::ResolvePathsResponse {
                    mount: mount_to_proto(&mount).into(),
                })
            }
            .await,
        )
    }

    async fn prefetch(
        &self,
        _ctx: RequestContext,
        request: ServiceRequest<'_, pb::PrefetchRequest>,
    ) -> Reply<pb::PrefetchResponse> {
        let r = request.to_owned_message();
        reply(
            async {
                let mount = self.daemon.mount(&r.name)?;
                let record = mount.record();
                let mut prefixes = paths(&r.prefixes)?;
                if prefixes.is_empty() {
                    prefixes.push(RepoPathBuf::root());
                }
                let base = mount.base();
                let mut files = Vec::new();
                for prefix in &prefixes {
                    files.extend(base.files(prefix, &record.sparse).await?);
                }
                let bytes: Vec<u64> = futures::stream::iter(files)
                    .map(|f| {
                        let mount = mount.clone();
                        async move {
                            mount.shared.content.get(&mount.repo, &f.content).await?;
                            Ok::<_, Error>(f.content.size())
                        }
                    })
                    .buffer_unordered(32)
                    .try_collect()
                    .await?;
                Ok(pb::PrefetchResponse {
                    files: bytes.len() as u64,
                    bytes: bytes.iter().sum(),
                })
            }
            .await,
        )
    }

    async fn shutdown(
        &self,
        _ctx: RequestContext,
        _request: ServiceRequest<'_, pb::ShutdownRequest>,
    ) -> Reply<pb::ShutdownResponse> {
        self.daemon.stop();
        reply(Ok(pb::ShutdownResponse::default()))
    }
}
