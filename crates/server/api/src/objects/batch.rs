//! `POST /o/{repo}/batch`: many objects in one length-prefixed stream.
//!
//! Request: a JSON array of `{"kind": "<kind>", "hash": "<hex>"}` (with
//! `"via": "<manifest hex>"` for a chunk), the read grant naming them in
//! the `x-rill-grant` header. Objects the grant does not name are reported
//! missing.
//! Response body (`application/vnd.rill.objects`): for each requested
//! object, in request order, one frame of
//!
//! ```text
//! kind     u8       the object kind number (see rill_types::ObjectKind)
//! hash     [u8;32]  the object hash
//! length   u64 BE   byte length, or u64::MAX when the object is missing
//! bytes    [u8]     the object (absent when missing)
//! ```
//!
//! Frames are produced lazily so a large batch never sits in memory at
//! once.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Extension, Json, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::{BufMut, Bytes, BytesMut};
use rill_policy::AuthContext;
use rill_types::{ObjectHash, ObjectKind};
use serde::Deserialize;

use super::authorize;
use super::handlers::parse_ref;
use super::{error_response, ObjectError};
use crate::services::common::opened;
use rill_runtime::state::AppState;

/// Content type of the batch response.
pub const BATCH_CONTENT_TYPE: &str = "application/vnd.rill.objects";

/// Sentinel length for a missing object.
pub const MISSING: u64 = u64::MAX;

/// Most objects one batch may request.
pub const MAX_BATCH: usize = 10_000;

/// One requested object.
#[derive(Debug, Deserialize)]
pub struct BatchItem {
    /// Object kind name (`blob`, `chunk`, `manifest`, `tree`, `cv`, `derived`).
    pub kind: String,
    /// Hex hash.
    pub hash: String,
    /// For a chunk, the manifest it is fetched through.
    #[serde(default)]
    pub via: Option<String>,
}

/// Encode one frame.
pub fn frame(kind: ObjectKind, hash: &ObjectHash, bytes: Option<&Bytes>) -> Bytes {
    let mut out = BytesMut::with_capacity(1 + 32 + 8 + bytes.map_or(0, |b| b.len()));
    out.put_u8(kind as u8);
    out.put_slice(hash.as_bytes());
    match bytes {
        Some(b) => {
            out.put_u64(b.len() as u64);
            out.put_slice(b);
        }
        None => out.put_u64(MISSING),
    }
    out.freeze()
}

/// The batch handler.
pub async fn batch(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<AuthContext>,
    Path(repo): Path<String>,
    headers: HeaderMap,
    Json(items): Json<Vec<BatchItem>>,
) -> Response {
    let prepared: Result<_, ObjectError> = async {
        if items.len() > MAX_BATCH {
            return Err(ObjectError::bad_request(format!(
                "batch of {} exceeds the limit of {MAX_BATCH}",
                items.len()
            )));
        }
        let refs = items
            .iter()
            .map(|i| parse_ref(&i.kind, &i.hash))
            .collect::<Result<Vec<_>, _>>()?;
        let repo = state.repos.resolve(&repo).await?;
        let opened = opened(&state, ctx, repo);
        opened.permit().read_any().await?;
        let grant = headers
            .get(rill_policy::grants::GRANT_HEADER)
            .and_then(|v| v.to_str().ok());
        let mut allowed = Vec::with_capacity(refs.len());
        for ((kind, hash), item) in refs.into_iter().zip(&items) {
            let ok = authorize::fetch(&opened, grant, kind, &hash, item.via.as_deref())
                .await
                .is_ok();
            allowed.push((kind, hash, ok));
        }
        Ok((opened.repo.clone(), allowed))
    }
    .await;
    let (repo, refs) = match prepared {
        Ok(p) => p,
        Err(e) => return error_response(e),
    };
    let stream = futures::stream::unfold((repo, refs.into_iter()), |(repo, mut refs)| async move {
        let (kind, hash, allowed) = refs.next()?;
        let bytes = match allowed {
            false => None,
            true => match repo.repo.get_bytes(kind, &hash).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, %kind, %hash, "batch read failed");
                    None
                }
            },
        };
        Some((
            Ok::<Bytes, std::convert::Infallible>(frame(kind, &hash, bytes.as_ref())),
            (repo, refs),
        ))
    });
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(BATCH_CONTENT_TYPE),
        )],
        Body::from_stream(stream),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let hash = ObjectHash::digest(ObjectKind::Blob, b"x");
        let f = frame(ObjectKind::Blob, &hash, Some(&Bytes::from_static(b"x")));
        assert_eq!(f[0], ObjectKind::Blob as u8);
        assert_eq!(&f[1..33], hash.as_bytes());
        assert_eq!(u64::from_be_bytes(f[33..41].try_into().unwrap()), 1);
        assert_eq!(&f[41..], b"x");
        let missing = frame(ObjectKind::Tree, &hash, None);
        assert_eq!(
            u64::from_be_bytes(missing[33..41].try_into().unwrap()),
            MISSING
        );
        assert_eq!(missing.len(), 41);
    }
}
