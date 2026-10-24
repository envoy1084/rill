//! Calling other servers for the search shards they serve.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use connectrpc::client::{ClientConfig, HttpClient};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ContentServiceClient;
use rill_search::{RankedFile, SearchHit, ShardResults};
use rill_types::RepoPathBuf;

use rill_runtime::error::{Error, Result};

/// Connections to search peers: one plaintext and one TLS pool.
pub(super) struct SearchPeers {
    ca_file: Option<PathBuf>,
    plain: HttpClient,
    tls: OnceLock<std::result::Result<HttpClient, String>>,
}

impl SearchPeers {
    pub(super) fn new(ca_file: Option<PathBuf>) -> SearchPeers {
        SearchPeers {
            ca_file,
            plain: HttpClient::plaintext_http2_only(),
            tls: OnceLock::new(),
        }
    }

    /// Search `shards` on the peer at `url`, as the caller whose
    /// `authorization` header is forwarded.
    pub(super) async fn grep_shards(
        &self,
        url: &str,
        authorization: Option<&http::HeaderValue>,
        grep: pb::GrepRequest,
        shards: &[usize],
        timeout: Duration,
    ) -> Result<ShardResults> {
        let uri: http::Uri = url
            .parse()
            .map_err(|e| Error::internal(format!("search peer {url:?}: {e}")))?;
        let http = match uri.scheme_str() {
            Some("https") => self
                .tls
                .get_or_init(|| tls_client(self.ca_file.as_deref()))
                .clone()
                .map_err(Error::internal)?,
            _ => self.plain.clone(),
        };
        let mut config = ClientConfig::new(uri).with_default_timeout(timeout);
        if let Some(value) = authorization.and_then(|v| v.to_str().ok()) {
            config = config.with_default_header("authorization", value.to_string());
        }
        let response = ContentServiceClient::new(http, config)
            .grep_shards(pb::GrepShardsRequest {
                grep: Some(grep).into(),
                shards: shards.iter().map(|&s| s as u32).collect(),
            })
            .await
            .map_err(|e| Error::Unavailable(format!("search peer {url}: {e}")))?
            .into_owned();
        from_proto(response)
    }
}

/// A peer's answer as shard results.
fn from_proto(response: pb::GrepShardsResponse) -> Result<ShardResults> {
    let path = |p: String| {
        RepoPathBuf::new(p).map_err(|e| Error::internal(format!("search peer path: {e}")))
    };
    let mut files = Vec::with_capacity(response.files.len());
    for f in response.files {
        let file = path(f.path)?;
        let hits = f
            .hits
            .into_iter()
            .map(|h| SearchHit {
                path: file.clone(),
                line_no: h.line_no,
                line: h.line,
                before: h.before,
                after: h.after,
                byte_range: h.byte_start as usize..h.byte_end as usize,
            })
            .collect();
        files.push(RankedFile {
            path: file,
            score: f.score,
            hits,
        });
    }
    Ok(ShardResults {
        files,
        capped: response.capped,
        partial: response.partial,
    })
}

/// A matched file as the wire carries it.
pub(super) fn file_to_proto(f: RankedFile) -> pb::GrepFile {
    pb::GrepFile {
        path: f.path.to_string(),
        score: f.score,
        hits: f.hits.into_iter().map(super::grep::hit_to_proto).collect(),
    }
}

/// TLS to peers: the system's roots plus `ca_file`.
fn tls_client(ca_file: Option<&std::path::Path>) -> std::result::Result<HttpClient, String> {
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::CertificateDer;
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        // A platform certificate rustls rejects is skipped, as clients do.
        let _ = roots.add(cert);
    }
    if let Some(ca) = ca_file {
        let err = |e: &dyn std::fmt::Display| format!("search peer CA {}: {e}", ca.display());
        for cert in CertificateDer::pem_file_iter(ca).map_err(|e| err(&e))? {
            roots.add(cert.map_err(|e| err(&e))?).map_err(|e| err(&e))?;
        }
    }
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec()];
    Ok(HttpClient::with_tls(Arc::new(config)))
}
