//! `GET`/`HEAD` of one object: parsing the request, checking its grant and
//! mapping errors to statuses.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use rill_policy::grants::GRANT_HEADER;
use rill_types::{ObjectHash, ObjectKind};
use serde::Deserialize;

use super::Cache;
use crate::error::{Error, Result};

/// Objects never change; each fetch still needs a grant, so only the
/// client may keep them.
const CACHE_CONTROL: &str = "private, immutable, max-age=31536000";

#[derive(Debug, Default, Deserialize)]
pub(super) struct FetchQuery {
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
        Error::Redacted(m) => refusal(
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
            format!("{m} was redacted"),
        ),
        Error::BadRequest(m) => refusal(StatusCode::BAD_REQUEST, m),
        Error::Unavailable(m) => {
            tracing::warn!(reason = %m, "request refused");
            refusal(StatusCode::SERVICE_UNAVAILABLE, m)
        }
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

pub(super) async fn get_object(
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

pub(super) async fn head_object(
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
