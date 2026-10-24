//! Code intelligence: symbols, definitions, references, SCIP uploads.

use bytes::Bytes;
use rill_proto::rill::v1 as pb;
use rill_types::RepoPath;

use super::RepoClient;
use crate::error::Result;
use crate::types::{ReferenceTarget, References, Root, ScipUpload, SymbolLocation};

fn locations(list: Vec<pb::SymbolLocation>) -> Result<Vec<SymbolLocation>> {
    Ok(list
        .into_iter()
        .map(SymbolLocation::try_from)
        .collect::<std::result::Result<_, _>>()?)
}

impl RepoClient {
    /// The symbols defined in the file at `path`.
    pub async fn file_symbols(&self, root: &Root, path: &RepoPath) -> Result<Vec<SymbolLocation>> {
        self.list_symbols(root, path.as_str(), "", &[], 0).await
    }

    /// Symbols whose names contain `query` (case-insensitive), of the
    /// given kinds (`function`, `class`, …; empty for all).
    pub async fn search_symbols(
        &self,
        root: &Root,
        query: &str,
        kinds: &[String],
        max: u32,
    ) -> Result<Vec<SymbolLocation>> {
        self.list_symbols(root, "", query, kinds, max).await
    }

    async fn list_symbols(
        &self,
        root: &Root,
        path: &str,
        query: &str,
        kinds: &[String],
        max: u32,
    ) -> Result<Vec<SymbolLocation>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::ListSymbolsRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
                path: path.into(),
                query: query.into(),
                kinds: kinds.to_vec(),
                max_results: max,
            };
            async move { svc.list_symbols(request).await }
        })
        .await?
        .into_owned();
        locations(resp.symbols)
    }

    /// Where `name` is defined: precise where a SCIP index covers the
    /// file, else from symbols extracted at indexing.
    pub async fn find_definitions(&self, root: &Root, name: &str) -> Result<Vec<SymbolLocation>> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::FindDefinitionsRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
                name: name.into(),
                max_results: 0,
            };
            async move { svc.find_definitions(request).await }
        })
        .await?
        .into_owned();
        locations(resp.definitions)
    }

    /// Uses of a symbol: precise from a SCIP index when one covers it,
    /// else whole-word matches of its name.
    pub async fn find_references(
        &self,
        root: &Root,
        target: &ReferenceTarget,
        max: u32,
    ) -> Result<References> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let mut request = pb::FindReferencesRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
                max_results: max,
                ..Default::default()
            };
            match target {
                ReferenceTarget::Name(n) => request.name = n.clone(),
                ReferenceTarget::Position { path, line, column } => {
                    request.path = path.to_string();
                    request.line = *line;
                    request.column = *column;
                }
            }
            async move { svc.find_references(request).await }
        })
        .await?
        .into_owned();
        Ok(References {
            references: locations(resp.references)?,
            precise: resp.precise,
            truncated: resp.truncated,
        })
    }

    /// Attach an encoded SCIP index, built from `root`, whose paths are
    /// relative to `prefix` (empty for the repository root).
    pub async fn upload_scip(&self, root: &Root, index: Bytes, prefix: &str) -> Result<ScipUpload> {
        let content = self.put_blob(index).await?;
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.content();
            let request = pb::UploadScipRequest {
                repo: self.resource.clone(),
                revision: root.to_string(),
                index: Some(pb::ContentRef::from(&content)).into(),
                prefix: prefix.into(),
            };
            async move { svc.upload_scip(request).await }
        })
        .await?
        .into_owned();
        Ok(ScipUpload {
            files: resp.files,
            occurrences: resp.occurrences,
        })
    }
}
