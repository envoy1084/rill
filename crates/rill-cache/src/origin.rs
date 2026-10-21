//! Fetching from the origin's object protocol with the client's own
//! credentials, so the origin decides exactly as if the client had asked.

use bytes::Bytes;
use rill_policy::grants::GRANT_HEADER;
use rill_types::{ObjectHash, ObjectKind, RepoId};

use crate::error::{Error, Result};

/// The origin server.
#[derive(Clone)]
pub struct Origin {
    base: String,
    client: reqwest::Client,
}

/// What a fetch presents.
pub struct Credentials<'a> {
    /// The client's `Authorization` header, passed through.
    pub authorization: Option<&'a str>,
    pub grant: &'a str,
    /// For a chunk, its manifest.
    pub via: Option<&'a ObjectHash>,
}

impl Origin {
    pub fn new(base: &str, client: reqwest::Client) -> Self {
        Origin {
            base: base.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// The object, or `None` when the origin has no such object.
    pub async fn fetch(
        &self,
        repo: RepoId,
        kind: ObjectKind,
        hash: &ObjectHash,
        creds: &Credentials<'_>,
    ) -> Result<Option<Bytes>> {
        let mut url = format!("{}/o/{repo}/{}/{}", self.base, kind.as_str(), hash.to_hex());
        if let Some(via) = creds.via {
            url.push_str(&format!("?via={}", via.to_hex()));
        }
        let mut request = self.client.get(url).header(GRANT_HEADER, creds.grant);
        if let Some(auth) = creds.authorization {
            request = request.header(reqwest::header::AUTHORIZATION, auth);
        }
        let response = request
            .send()
            .await
            .map_err(|e| Error::Origin(e.to_string()))?;
        match response.status().as_u16() {
            200 => Ok(Some(
                response
                    .bytes()
                    .await
                    .map_err(|e| Error::Origin(e.to_string()))?,
            )),
            404 => Ok(None),
            401 | 403 => Err(Error::Denied(
                response
                    .text()
                    .await
                    .unwrap_or_else(|_| "refused by the origin".into()),
            )),
            code => Err(Error::Origin(format!("{code} for {kind} {hash}"))),
        }
    }
}
