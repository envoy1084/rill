//! The object protocol at the cache tier: `GET`/`HEAD /o/{repo}/{kind}/{hash}`
//! with a read grant in `x-rill-grant` (or `?grant=`), and `?via=` for a
//! chunk. The grant decides; the `{repo}` segment is the client's name for
//! the repository and is not trusted. A grant is a bearer credential here:
//! whoever holds it may fetch what it names until it expires.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use rill_policy::grants::{ReadGrant, GRANT_HEADER};
use rill_types::{BlobManifest, Encodable, ObjectHash, ObjectKind};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::coalesce::Coalescer;
use crate::config::CacheConfig;
use crate::disk::DiskCache;
use crate::error::{Error, Result};
use crate::keys::OriginKeys;
use crate::origin::{Credentials, Origin};

/// Counters a cache keeps.
#[derive(Debug, Default)]
pub struct CacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    /// Misses served by another request's origin fetch.
    pub coalesced: AtomicU64,
    pub origin_fetches: AtomicU64,
    pub denied: AtomicU64,
}

/// A running cache.
pub struct Cache {
    disk: Arc<DiskCache>,
    keys: Arc<OriginKeys>,
    origin: Origin,
    inflight: Coalescer<(ObjectKind, ObjectHash), Bytes>,
    stats: CacheStats,
}

/// Objects never change; each fetch still needs a grant, so only the
/// client may keep them.
const CACHE_CONTROL: &str = "private, immutable, max-age=31536000";

impl Cache {
    /// Open the disk store and read the origin's grant keys.
    pub async fn start(config: &CacheConfig) -> Result<Arc<Self>> {
        let client = reqwest::Client::new();
        let disk = DiskCache::open(&config.dir, config.max_bytes).await?;
        let keys = OriginKeys::fetch(&config.origin, client.clone()).await?;
        Ok(Arc::new(Cache {
            disk,
            keys,
            origin: Origin::new(&config.origin, client),
            inflight: Coalescer::new(),
            stats: CacheStats::default(),
        }))
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn disk(&self) -> &DiskCache {
        &self.disk
    }

    /// The object, from disk or the origin, once `grant` is seen to name
    /// it.
    async fn object(
        &self,
        grant: &ReadGrant,
        grant_text: &str,
        authorization: Option<&str>,
        kind: ObjectKind,
        hash: &ObjectHash,
        via: Option<&ObjectHash>,
    ) -> Result<Bytes> {
        match kind {
            ObjectKind::Chunk => {
                let via = via.ok_or_else(|| {
                    Error::Denied("a chunk is fetched through its manifest (`via`)".into())
                })?;
                let manifest = Box::pin(self.object(
                    grant,
                    grant_text,
                    authorization,
                    ObjectKind::BlobManifest,
                    via,
                    None,
                ))
                .await?;
                let listed = BlobManifest::decode(&manifest)
                    .map_err(|e| Error::Origin(format!("manifest {via}: {e}")))?;
                if !listed.chunks.iter().any(|c| c.hash == *hash) {
                    return Err(Error::Denied(format!(
                        "manifest {via} does not list chunk {hash}"
                    )));
                }
            }
            ObjectKind::Derived => {
                return Err(Error::Denied("derived values are not served here".into()))
            }
            _ => {
                if grant.entry(kind, hash).is_none() {
                    return Err(Error::Denied(format!(
                        "the read grant does not name {kind} {hash}"
                    )));
                }
            }
        }
        if let Some(bytes) = self.disk.get(kind, hash).await? {
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(bytes);
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        let creds = Credentials {
            authorization,
            grant: grant_text,
            via,
        };
        let (result, shared) = self
            .inflight
            .get((kind, *hash), || self.fill(grant, kind, hash, &creds))
            .await;
        if shared {
            self.stats.coalesced.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    async fn fill(
        &self,
        grant: &ReadGrant,
        kind: ObjectKind,
        hash: &ObjectHash,
        creds: &Credentials<'_>,
    ) -> Result<Bytes> {
        self.stats.origin_fetches.fetch_add(1, Ordering::Relaxed);
        let bytes = self
            .origin
            .fetch(grant.repo, kind, hash, creds)
            .await?
            .ok_or_else(|| Error::NotFound(format!("{kind} {hash}")))?;
        rill_store::object::verify(kind, hash, &bytes)
            .map_err(|e| Error::Corrupt(e.to_string()))?;
        if let Err(e) = self.disk.put(kind, hash, bytes.clone()).await {
            tracing::warn!(error = %e, %kind, %hash, "keeping an object on disk failed");
        }
        Ok(bytes)
    }
}

#[derive(Debug, Default, Deserialize)]
struct FetchQuery {
    grant: Option<String>,
    via: Option<String>,
}

fn refusal(status: StatusCode, message: String) -> Response {
    (status, message).into_response()
}

fn error_response(cache: &Cache, e: Error) -> Response {
    match e {
        Error::Denied(m) => {
            cache.stats.denied.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(reason = %m, "request refused");
            refusal(StatusCode::FORBIDDEN, m)
        }
        Error::NotFound(m) => refusal(StatusCode::NOT_FOUND, m),
        Error::BadRequest(m) => refusal(StatusCode::BAD_REQUEST, m),
        Error::Origin(m) | Error::Corrupt(m) => {
            tracing::warn!(error = %m, "origin fetch failed");
            refusal(StatusCode::BAD_GATEWAY, m)
        }
        other => refusal(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}

async fn fetch(
    cache: &Cache,
    kind: &str,
    hash: &str,
    query: &FetchQuery,
    headers: &HeaderMap,
) -> Result<(ObjectHash, Bytes)> {
    let kind: ObjectKind = kind
        .parse()
        .map_err(|_| Error::BadRequest(format!("unknown object kind {kind:?}")))?;
    let hash = ObjectHash::from_hex(hash)
        .map_err(|_| Error::BadRequest(format!("invalid object hash {hash:?}")))?;
    let via = query
        .via
        .as_deref()
        .map(ObjectHash::from_hex)
        .transpose()
        .map_err(|_| Error::BadRequest("invalid via hash".into()))?;
    let grant_text = headers
        .get(GRANT_HEADER)
        .and_then(|v| v.to_str().ok())
        .or(query.grant.as_deref())
        .filter(|g| !g.is_empty())
        .ok_or_else(|| Error::Denied("fetching an object needs a read grant".into()))?;
    let grant = cache.keys.verify(grant_text).await?;
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let bytes = cache
        .object(&grant, grant_text, authorization, kind, &hash, via.as_ref())
        .await?;
    Ok((hash, bytes))
}

fn object_headers(hash: &ObjectHash, len: usize) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(etag) = HeaderValue::from_str(&format!("\"{hash}\"")) {
        headers.insert(header::ETAG, etag);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL),
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    headers
}

async fn get_object(
    State(cache): State<Arc<Cache>>,
    Path((_repo, kind, hash)): Path<(String, String, String)>,
    Query(query): Query<FetchQuery>,
    headers: HeaderMap,
) -> Response {
    match fetch(&cache, &kind, &hash, &query, &headers).await {
        Ok((hash, bytes)) => {
            (StatusCode::OK, object_headers(&hash, bytes.len()), bytes).into_response()
        }
        Err(e) => error_response(&cache, e),
    }
}

async fn head_object(
    State(cache): State<Arc<Cache>>,
    Path((_repo, kind, hash)): Path<(String, String, String)>,
    Query(query): Query<FetchQuery>,
    headers: HeaderMap,
) -> Response {
    match fetch(&cache, &kind, &hash, &query, &headers).await {
        Ok((hash, bytes)) => (StatusCode::OK, object_headers(&hash, bytes.len())).into_response(),
        Err(e) => error_response(&cache, e),
    }
}

async fn metrics(State(cache): State<Arc<Cache>>) -> Response {
    let s = &cache.stats;
    let mut out = String::new();
    for (name, help, value) in [
        (
            "rill_cache_hits",
            "Objects served from disk.",
            s.hits.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_misses",
            "Objects not on disk.",
            s.misses.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_coalesced",
            "Misses served by another request's fetch.",
            s.coalesced.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_origin_fetches",
            "Fetches from the origin.",
            s.origin_fetches.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_denied",
            "Requests refused for want of a grant.",
            s.denied.load(Ordering::Relaxed),
        ),
        (
            "rill_cache_evictions",
            "Objects evicted from disk.",
            cache.disk.evictions(),
        ),
    ] {
        out.push_str(&format!(
            "# HELP {name} {help}\n# TYPE {name} counter\n{name}_total {value}\n"
        ));
    }
    out.push_str(&format!(
        "# HELP rill_cache_bytes Bytes on disk.\n# TYPE rill_cache_bytes gauge\nrill_cache_bytes {}\n# EOF\n",
        cache.disk.bytes()
    ));
    (
        [(
            header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        out,
    )
        .into_response()
}

/// The cache's routes.
pub fn router(cache: Arc<Cache>) -> Router {
    Router::new()
        .route("/o/{repo}/{kind}/{hash}", get(get_object).head(head_object))
        .route("/metrics", get(metrics))
        .route("/livez", get(|| async { "ok" }))
        .with_state(cache)
}

/// Serve `cache` on `listener` until `shutdown`, rereading the origin's
/// grant keys every `key_refresh`.
pub async fn serve_on(
    cache: Arc<Cache>,
    listener: tokio::net::TcpListener,
    key_refresh: std::time::Duration,
    shutdown: CancellationToken,
) -> Result<()> {
    let keys = cache.keys.clone();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop.cancelled() => return,
                _ = tokio::time::sleep(key_refresh) => {}
            }
            if let Err(e) = keys.refresh().await {
                tracing::warn!(error = %e, "rereading grant keys failed");
            }
        }
    });
    axum::serve(listener, router(cache))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .map_err(Error::Io)
}

/// Run a cache from `config` until `shutdown`.
pub async fn serve(config: CacheConfig, shutdown: CancellationToken) -> Result<()> {
    let cache = Cache::start(&config).await?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let addr: SocketAddr = listener.local_addr()?;
    tracing::info!(%addr, origin = %config.origin, dir = %config.dir.display(), max_bytes = config.max_bytes, "cache serving");
    serve_on(cache, listener, config.key_refresh, shutdown).await
}
