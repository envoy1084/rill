//! Secret scanning of every version as it is committed, under the rules
//! on trunk (`.rill/secrets.toml`, block by default).

use std::sync::Arc;

use dashmap::DashMap;
use futures::StreamExt;
use rill_policy::secrets::{scan, Finding, SecretRules, MAX_FINDINGS, RULES_PATH};
use rill_types::limits::MAX_FILES_PER_CHANGE;
use rill_types::{
    ChangeVersion, ChangeVersionHash, ContentRef, EntryKind, FileChange, RepoPathBuf,
    SparseProfile, SubtreeChange,
};

use crate::error::Result;
use crate::state::RepoContext;

/// Findings reported for one version; scanning stops there.
pub const MAX_VERSION_FINDINGS: usize = 100;
/// The path findings in a version's message, paths or extension fields
/// are reported at.
const METADATA_PATH: &str = "(message)";
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

    /// The rules in force and the findings in what `cv` adds: its message,
    /// paths and extension fields, the files it sets, and the files of every
    /// directory it copies, each at its destination path. The caller has
    /// checked that `cv`'s author may read what it copies.
    pub async fn version(&self, ctx: &RepoContext, cv: &ChangeVersion) -> Result<VersionScan> {
        let (rules, digest) = self.rules(ctx).await?;
        let copies = copied_trees(ctx, cv).await?;
        let mut scan = VersionScan {
            findings: Vec::new(),
            truncated: false,
            rules: rules.clone(),
        };
        let found = scan_off_thread(METADATA_PATH.to_string(), metadata(cv), rules.clone()).await?;
        if scan.add(found) {
            return Ok(scan);
        }
        for (path, change) in &cv.file_changes {
            let contents: Vec<&ContentRef> = match change {
                FileChange::Set { content, .. } => vec![content],
                FileChange::Conflict(terms) => {
                    terms.as_slice().iter().flatten().map(|(c, _)| c).collect()
                }
                FileChange::Delete => Vec::new(),
            };
            for content in contents {
                if scan.add(self.content(ctx, path, content, &rules, digest).await?) {
                    return Ok(scan);
                }
            }
        }
        for (dest, from_path, root) in copies {
            let files = ctx
                .trees
                .walk(root, &from_path, &SparseProfile::default())
                .await?;
            for file in files {
                let rel = file
                    .path
                    .strip_prefix(&from_path)
                    .map(|r| r.as_str().to_string())
                    .unwrap_or_default();
                let path = if rel.is_empty() {
                    dest.clone()
                } else if dest.is_root() {
                    RepoPathBuf::new(&rel)?
                } else {
                    RepoPathBuf::new(format!("{dest}/{rel}"))?
                };
                if scan.add(
                    self.content(ctx, &path, &file.content, &rules, digest)
                        .await?,
                ) {
                    return Ok(scan);
                }
            }
        }
        Ok(scan)
    }

    async fn content(
        &self,
        ctx: &RepoContext,
        path: &RepoPathBuf,
        content: &ContentRef,
        rules: &Arc<SecretRules>,
        digest: [u8; 32],
    ) -> Result<Vec<Finding>> {
        // Content at an allowed path is not looked at, so it is not known
        // clean: the same bytes elsewhere are scanned.
        if rules.path_allowed(path.as_str()) {
            return Ok(Vec::new());
        }
        let path = path.as_str().to_string();
        let key = match content {
            ContentRef::Inline { bytes } => {
                return scan_off_thread(path, bytes.to_vec(), rules.clone()).await;
            }
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
            let mut window = std::mem::take(&mut carry);
            window.extend_from_slice(&part?);
            let (path, rules) = (path.clone(), rules.clone());
            let (found, window, lines) = tokio::task::spawn_blocking(move || {
                let found = scan(&path, &window, &rules);
                let keep = window.len().min(OVERLAP);
                let lines = window[..window.len() - keep]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count();
                (found, window, lines)
            })
            .await
            .map_err(|e| crate::error::Error::Internal(format!("secret scan: {e}")))?;
            for mut f in found {
                if !findings.iter().any(|g| g.fingerprint == f.fingerprint) {
                    f.line += lines_before;
                    findings.push(f);
                }
            }
            if findings.len() >= MAX_FINDINGS {
                findings.truncate(MAX_FINDINGS);
                return Ok(findings);
            }
            lines_before += lines;
            let keep = window.len().min(OVERLAP);
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

/// What scanning a version found.
pub struct VersionScan {
    pub rules: Arc<SecretRules>,
    pub findings: Vec<Finding>,
    /// Scanning stopped at a limit, so there may be more than were found.
    pub truncated: bool,
}

impl VersionScan {
    /// Add one file's findings; true once the version's limit is reached.
    fn add(&mut self, found: Vec<Finding>) -> bool {
        self.truncated |= found.len() >= MAX_FINDINGS;
        self.findings.extend(found);
        if self.findings.len() >= MAX_VERSION_FINDINGS {
            self.findings.truncate(MAX_VERSION_FINDINGS);
            self.truncated = true;
        }
        self.truncated && self.findings.len() >= MAX_VERSION_FINDINGS
    }
}

/// The free text `cv` carries besides file content: its message, the
/// paths it touches and its extension fields.
fn metadata(cv: &ChangeVersion) -> Vec<u8> {
    let mut text = cv.message.clone().into_bytes();
    for path in cv.file_changes.keys().chain(cv.subtree_changes.keys()) {
        text.push(b'\n');
        text.extend_from_slice(path.as_str().as_bytes());
    }
    for (key, value) in &cv.extra {
        text.push(b'\n');
        text.extend_from_slice(key.as_bytes());
        text.extend_from_slice(b" = ");
        text.extend_from_slice(value);
    }
    text
}

/// The directories `cv` copies, as `(destination, source path, source
/// root)`, once their files are known to fit the per-change file limit.
async fn copied_trees(
    ctx: &RepoContext,
    cv: &ChangeVersion,
) -> Result<Vec<(RepoPathBuf, RepoPathBuf, rill_types::TreeHash)>> {
    let mut out = Vec::new();
    let mut files = cv.file_changes.len() as u64;
    for (dest, change) in &cv.subtree_changes {
        let SubtreeChange::Copy {
            from_path,
            from_version,
        } = change
        else {
            continue;
        };
        let root = ctx.derive.get::<rill_core::RootTree>(from_version).await?;
        files += if from_path.is_root() {
            ctx.trees.get_tree(&root).await?.summary().file_count
        } else {
            match ctx.trees.lookup(root, from_path).await? {
                Some(EntryKind::Dir { summary, .. }) => summary.file_count,
                Some(EntryKind::File { .. }) => 1,
                None => 0,
            }
        };
        if files > MAX_FILES_PER_CHANGE as u64 {
            return Err(crate::error::Error::invalid(format!(
                "the version changes or copies more than {MAX_FILES_PER_CHANGE} files"
            )));
        }
        out.push((dest.clone(), from_path.clone(), root));
    }
    Ok(out)
}

/// Scan `bytes` on the blocking pool: content can be large.
async fn scan_off_thread(
    path: String,
    bytes: Vec<u8>,
    rules: Arc<SecretRules>,
) -> Result<Vec<Finding>> {
    tokio::task::spawn_blocking(move || scan(&path, &bytes, &rules))
        .await
        .map_err(|e| crate::error::Error::Internal(format!("secret scan: {e}")))
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
