//! Secret scanning of every version as it is committed, under the rules
//! on trunk (`.rill/secrets.toml`, block by default).

use std::sync::Arc;

use dashmap::DashMap;
use futures::StreamExt;
use rill_policy::secrets::{scan, Finding, SecretRules, RULES_PATH};
use rill_types::{
    ChangeVersion, ChangeVersionHash, ContentRef, EntryKind, FileChange, RepoPathBuf,
};

use crate::error::Result;
use crate::state::RepoContext;

/// Content found clean is not scanned again under the same rules.
const CLEAN_CAPACITY: usize = 200_000;
/// Bytes carried from one part of a large file into the next, so a
/// secret split across two stored chunks is still found.
const OVERLAP: usize = 4096;

/// Rules read at a trunk version, with a digest of their text.
type CachedRules = (Option<ChangeVersionHash>, Arc<SecretRules>, [u8; 32]);

/// A repository's rules (by the trunk version they came from) and the
/// content already found clean under them.
#[derive(Default)]
pub struct Scanner {
    rules: parking_lot::Mutex<Option<CachedRules>>,
    clean: DashMap<([u8; 32], [u8; 32]), ()>,
}

impl Scanner {
    async fn rules(&self, ctx: &RepoContext) -> Result<(Arc<SecretRules>, [u8; 32])> {
        let head = ctx.repo.trunk_head().await?.map(|(_, h)| h);
        if let Some((at, rules, digest)) = self.rules.lock().as_ref() {
            if *at == head {
                return Ok((rules.clone(), *digest));
            }
        }
        let text = match head {
            None => String::new(),
            Some(version) => {
                let root = ctx.derive.get::<rill_core::RootTree>(&version).await?;
                match ctx
                    .trees
                    .lookup(root, &RepoPathBuf::new(RULES_PATH)?)
                    .await?
                {
                    Some(EntryKind::File { content, .. }) => String::from_utf8_lossy(
                        &rill_core::read_content(ctx.repo.objects().as_ref(), &content).await?,
                    )
                    .into_owned(),
                    _ => String::new(),
                }
            }
        };
        // Broken rules scan with the defaults (block, built-in detectors)
        // rather than not at all.
        let rules = Arc::new(SecretRules::parse(&text).unwrap_or_else(|e| {
            tracing::warn!(repo = %ctx.config.name, error = %e, "unreadable secret rules; using the defaults");
            SecretRules::default()
        }));
        let digest = *blake3::hash(text.as_bytes()).as_bytes();
        *self.rules.lock() = Some((head, rules.clone(), digest));
        Ok((rules, digest))
    }

    /// The rules in force and the findings in `cv`'s new content.
    pub async fn version(
        &self,
        ctx: &RepoContext,
        cv: &ChangeVersion,
    ) -> Result<(Arc<SecretRules>, Vec<Finding>)> {
        let (rules, digest) = self.rules(ctx).await?;
        let mut findings = Vec::new();
        for (path, change) in &cv.file_changes {
            let contents: Vec<&ContentRef> = match change {
                FileChange::Set { content, .. } => vec![content],
                FileChange::Conflict(terms) => {
                    terms.as_slice().iter().flatten().map(|(c, _)| c).collect()
                }
                FileChange::Delete => Vec::new(),
            };
            for content in contents {
                findings.extend(self.content(ctx, path, content, &rules, digest).await?);
            }
        }
        Ok((rules, findings))
    }

    async fn content(
        &self,
        ctx: &RepoContext,
        path: &RepoPathBuf,
        content: &ContentRef,
        rules: &SecretRules,
        digest: [u8; 32],
    ) -> Result<Vec<Finding>> {
        let path = path.as_str();
        let key = match content {
            ContentRef::Inline { bytes } => return Ok(scan(path, bytes, rules)),
            ContentRef::Blob { hash, .. } | ContentRef::Chunked { hash, .. } => {
                (*hash.as_bytes(), digest)
            }
        };
        if self.clean.contains_key(&key) {
            return Ok(Vec::new());
        }
        let mut parts =
            rill_core::content_parts(ctx.repo.objects().clone(), content, 0, u64::MAX).await?;
        let mut carry: Vec<u8> = Vec::new();
        let mut lines_before = 0usize;
        let mut findings: Vec<Finding> = Vec::new();
        while let Some(part) = parts.next().await {
            let part = part?;
            let mut window = std::mem::take(&mut carry);
            window.extend_from_slice(&part);
            for mut f in scan(path, &window, rules) {
                if !findings.iter().any(|g| g.fingerprint == f.fingerprint) {
                    f.line += lines_before;
                    findings.push(f);
                }
            }
            let keep = window.len().min(OVERLAP);
            lines_before += window[..window.len() - keep]
                .iter()
                .filter(|b| **b == b'\n')
                .count();
            carry = window[window.len() - keep..].to_vec();
        }
        if findings.is_empty() {
            if self.clean.len() > CLEAN_CAPACITY {
                self.clean.clear();
            }
            self.clean.insert(key, ());
        }
        Ok(findings)
    }
}

/// How a refusal names what was found, without the secret.
pub fn describe(findings: &[Finding]) -> String {
    findings
        .iter()
        .take(5)
        .map(|f| {
            format!(
                "{} at {}:{} (fingerprint {})",
                f.detector, f.path, f.line, f.fingerprint
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}
