//! Single-object GET, HEAD and PUT handlers.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rill_policy::AuthContext;
use rill_types::{ObjectHash, ObjectKind};

use super::authorize::{self, grant_of, FetchQuery};
use super::{error_response, ObjectError};
use crate::services::common::opened;
use rill_runtime::state::AppState;

/// Objects never change, so a client may keep them for a year; shared
/// caches must not, since each fetch needs a read grant (the Rill cache
/// tier checks grants itself).
pub const CACHE_CONTROL: &str = "private, immutable, max-age=31536000";

/// Parse the `{kind}/{hash}` path segments.
pub fn parse_ref(kind: &str, hash: &str) -> Result<(ObjectKind, ObjectHash), ObjectError> {
    let kind: ObjectKind = kind
        .parse()
        .map_err(|_| ObjectError::bad_request(format!("unknown object kind {kind:?}")))?;
    let hash = ObjectHash::from_hex(hash)
        .map_err(|_| ObjectError::bad_request(format!("invalid object hash {hash:?}")))?;
    Ok((kind, hash))
}

fn object_headers(hash: &ObjectHash, len: usize) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{hash}\"")).expect("hex etag is a valid header"),
    );
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

/// `GET /o/{repo}/{kind}/{hash}`: the raw object bytes.
pub async fn get_object(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<AuthContext>,
    Path((repo, kind, hash)): Path<(String, String, String)>,
    Query(query): Query<FetchQuery>,
    headers: HeaderMap,
) -> Response {
    match fetch(&state, ctx, &repo, &kind, &hash, &headers, &query).await {
        Ok((hash, bytes)) => {
            (StatusCode::OK, object_headers(&hash, bytes.len()), bytes).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// `HEAD /o/{repo}/{kind}/{hash}`: headers only.
pub async fn head_object(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<AuthContext>,
    Path((repo, kind, hash)): Path<(String, String, String)>,
    Query(query): Query<FetchQuery>,
    headers: HeaderMap,
) -> Response {
    match fetch(&state, ctx, &repo, &kind, &hash, &headers, &query).await {
        Ok((hash, bytes)) => (StatusCode::OK, object_headers(&hash, bytes.len())).into_response(),
        Err(e) => error_response(e),
    }
}

async fn fetch(
    state: &AppState,
    ctx: AuthContext,
    repo: &str,
    kind: &str,
    hash: &str,
    headers: &HeaderMap,
    query: &FetchQuery,
) -> Result<(ObjectHash, Bytes), ObjectError> {
    let (kind, hash) = parse_ref(kind, hash)?;
    let repo = state.repos.resolve(repo).await?;
    let opened = opened(state, ctx, repo);
    authorize::fetch(
        &opened,
        grant_of(headers, query),
        kind,
        &hash,
        query.via.as_deref(),
    )
    .await?;
    let bytes = opened
        .repo
        .repo
        .get_bytes(kind, &hash)
        .await?
        .ok_or_else(|| ObjectError::not_found(format!("{kind} {hash}")))?;
    Ok((hash, bytes))
}

/// `PUT /o/{repo}/{kind}/{hash}`: store an object. The body must hash to
/// `{hash}` under `{kind}`'s domain, or the request is rejected with 400.
pub async fn put_object(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<AuthContext>,
    Path((repo, kind, hash)): Path<(String, String, String)>,
    body: Bytes,
) -> Response {
    let result: Result<StatusCode, ObjectError> = async {
        let (kind, hash) = parse_ref(&kind, &hash)?;
        let repo = state.repos.resolve(&repo).await?;
        let opened = opened(&state, ctx, repo);
        // The commit naming this object checks its path.
        opened.permit().write_any().await?;
        rill_store::object::verify(kind, &hash, &body)
            .map_err(|e| ObjectError::bad_request(e.to_string()))?;
        let existed = opened.repo.repo.objects().has(kind, &hash).await?;
        opened.repo.repo.objects().put(kind, &hash, body).await?;
        Ok(if existed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        })
    }
    .await;
    match result {
        Ok(status) => status.into_response(),
        Err(e) => error_response(e),
    }
}
