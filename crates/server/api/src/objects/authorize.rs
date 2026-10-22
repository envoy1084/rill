//! Who may fetch an object by hash: a caller whose read grant names it.

use axum::http::HeaderMap;
use rill_policy::grants::GRANT_HEADER;
use rill_types::{ObjectHash, ObjectKind, RepoPathBuf};
use serde::Deserialize;

use super::ObjectError;
use crate::grants;
use crate::services::common::Opened;

/// `?grant=` and `?via=` on object fetches.
#[derive(Debug, Default, Deserialize)]
pub struct FetchQuery {
    /// The read grant, when not sent as the `x-rill-grant` header.
    pub grant: Option<String>,
    /// For a chunk: the manifest it is fetched through.
    pub via: Option<String>,
}

/// The grant a request carries: the header, else the query parameter.
pub fn grant_of<'a>(headers: &'a HeaderMap, query: &'a FetchQuery) -> Option<&'a str> {
    headers
        .get(GRANT_HEADER)
        .and_then(|v| v.to_str().ok())
        .or(query.grant.as_deref())
}

/// Allow the fetch of `(kind, hash)`, or refuse it.
pub async fn fetch(
    opened: &Opened,
    grant: Option<&str>,
    kind: ObjectKind,
    hash: &ObjectHash,
    via: Option<&str>,
) -> Result<(), ObjectError> {
    match kind {
        ObjectKind::Chunk => {
            let via = via
                .map(ObjectHash::from_hex)
                .transpose()
                .map_err(|_| ObjectError::bad_request("invalid via hash"))?;
            grants::require_chunk(opened, grant, hash, via.as_ref()).await?;
        }
        // Derived values are computed over whole trees.
        ObjectKind::Derived => opened.permit().read(&RepoPathBuf::root()).await?,
        _ => {
            grants::require(opened, grant, kind, hash)?;
        }
    }
    Ok(())
}
