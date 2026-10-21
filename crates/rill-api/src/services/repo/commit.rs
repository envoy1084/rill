//! Storing a change version: everything a commit makes visible commits in
//! one transaction.
//!
//! The version object and its land-critical derived values are written
//! first; they are immutable, so writing them early is harmless. Then one
//! transaction stores the version's rows, its signatures (with the
//! server's attestation), the review update, an event, a quota charge, the
//! caller's extra writes (a workspace operation, say) and, for a request
//! with an operation id, a note in its claim of which version it created.
//! A retry after a crash finds that note and returns the same version.

use rill_review::error::Error as ReviewError;
use rill_store::meta::Transaction;
use rill_store::repo::metric;
use rill_types::{
    ChangeVersion, ChangeVersionHash, Encodable, Event, EventKind, FileChange, ObjectHash,
    Signature, Timestamp,
};

use super::trees::check_content_exists;
use crate::error::{Error, Result};
use crate::services::common::Opened;
use crate::services::idem;

/// Attempts at the commit transaction when the review moves under it.
const COMMIT_ATTEMPTS: usize = 8;

/// What commits in the same transaction as a version, besides what every
/// commit writes.
#[derive(Debug, Default)]
pub struct CommitExtras {
    /// Client signatures, stored next to the server's attestation.
    pub signatures: Vec<Signature>,
    /// Further writes, such as a workspace operation.
    pub tx: Transaction,
}

/// Validate and store a change version.
pub async fn commit_version(
    opened: &Opened,
    cv: ChangeVersion,
) -> Result<(ChangeVersionHash, ChangeVersion)> {
    commit_version_with(opened, cv, CommitExtras::default()).await
}

/// [`commit_version`] with writes that must commit with the version.
pub async fn commit_version_with(
    opened: &Opened,
    mut cv: ChangeVersion,
    extras: CommitExtras,
) -> Result<(ChangeVersionHash, ChangeVersion)> {
    let ctx = &opened.repo;
    super::stamp::stamp_provenance(&mut cv, &opened.ctx);
    let scope = idem::current();
    if let Some(prior) = scope.as_ref().and_then(|s| s.prior.as_ref()) {
        if let Ok(hash) = ObjectHash::from_slice(prior) {
            if let Some(stored) = ctx.repo.get_change_version(&hash).await? {
                return Ok((hash, stored));
            }
        }
    }
    validate(opened, &cv).await?;
    let hash = cv.hash();
    // Scanning reads what the version copies and names what it finds, so
    // the author's rights are checked first.
    super::authorize::version(opened, &cv).await?;
    let found = secret_findings(opened, &cv, &hash).await?;
    let staged = ctx.repo.stage_version(&cv).await?;
    // Derive what landing needs now, so a bad change fails the request and
    // the lander never derives it.
    ctx.derive.ensure_land_critical(&hash).await?;
    let mut signatures = vec![ctx
        .attestor
        .sign(&hash, &rill_types::Principal::service("rill-server"))];
    signatures.extend(extras.signatures);
    let stores = ctx.repo.stores();

    for _ in 0..COMMIT_ATTEMPTS {
        let relations = super::authorize::version(opened, &cv).await?;
        let prepared = match ctx.review().prepare_version(&cv.change_id, hash).await {
            Ok(prepared) => prepared,
            Err(ReviewError::ReviewNotFound(_)) => None,
            Err(ReviewError::InvalidStatus { status, .. }) => {
                tracing::debug!(change = %cv.change_id, ?status, "review not updated with new version");
                None
            }
            Err(e) => return Err(e.into()),
        };
        let now = Timestamp::now();
        let event = |kind| Event {
            kind,
            actor: opened.principal().clone(),
            at: now,
        };
        let mut tx = staged.clone().merge(extras.tx.clone());
        tx.mutations.extend(found.clone());
        if !opened.dev_mode() {
            tx = ctx.repo.expect_relations_version(tx, relations);
        }
        if let Some(spend) =
            crate::services::budget::spend(opened, crate::services::budget::VERSIONS).await?
        {
            tx = tx.merge(spend);
        }
        tx.mutations
            .push(ctx.repo.signatures_mutation(&hash, &signatures)?);
        tx.mutations
            .extend(ctx.repo.event(&event(EventKind::VersionStored {
                version: hash,
                change: cv.change_id,
            }))?);
        tx.mutations
            .push(stores.charge(opened.principal(), metric::VERSIONS, 1));
        if let Some(p) = &prepared {
            tx = tx.merge(
                ctx.repo
                    .review_writes(&p.prev, &p.next, opened.principal())?,
            );
        }
        if let Some(s) = &scope {
            tx.mutations.push(stores.idem_effect(
                &s.principal,
                &s.operation,
                s.digest,
                hash.as_bytes(),
            )?);
        }
        match ctx.repo.meta().transact(tx).await {
            Ok(_) => {
                if let Some(p) = prepared {
                    ctx.review().version_written(p);
                }
                return Ok((hash, cv));
            }
            Err(e) if e.is_conflict() => {
                if ctx.repo.has_change_version(&hash).await? {
                    // An identical commit won; it carried its own review
                    // update and extras.
                    return Ok((hash, cv));
                }
                // The relations may have changed under the decision.
                ctx.authorizer.invalidate();
                tracing::debug!(change = %cv.change_id, "commit conflict; retrying");
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::FailedPrecondition(format!(
        "change {} kept changing while the version was stored; retry",
        cv.change_id
    )))
}

/// Findings named in one audit record.
const AUDITED_FINDINGS: usize = 20;

/// Scan `cv` for credentials. A finding refuses the version unless the
/// repository's rules say warn or quarantine, in which case the returned
/// mutations (the audit record, and the quarantine) commit with it.
async fn secret_findings(
    opened: &Opened,
    cv: &ChangeVersion,
    hash: &ChangeVersionHash,
) -> Result<Vec<rill_store::meta::Mutation>> {
    use rill_policy::secrets::ScanMode;
    use rill_types::audit::{AuditOutcome, AuditRecord};

    let ctx = &opened.repo;
    let scan = ctx.secrets.version(ctx, cv).await?;
    let (rules, findings) = (scan.rules, scan.findings);
    if findings.is_empty() {
        return Ok(Vec::new());
    }
    let mut described = super::secrets::describe(&findings);
    // Too many to review: an owner releasing a quarantine would see only
    // the first few, so the version is refused whatever the mode.
    let mode = if scan.truncated {
        described.push_str(", and more not listed");
        ScanMode::Block
    } else {
        rules.mode
    };
    let audit = ctx.repo.stores().audit();
    let record = |action: &str, outcome: AuditOutcome| AuditRecord {
        org: ctx.config.org.clone(),
        at: Timestamp::now(),
        actor: opened.principal().clone(),
        action: action.to_string(),
        target: format!("repo:{}", ctx.config.name),
        outcome,
        detail: findings
            .iter()
            .take(AUDITED_FINDINGS)
            .enumerate()
            .map(|(i, f)| {
                (
                    format!("finding.{i}"),
                    format!("{} {}:{} {}", f.detector, f.path, f.line, f.fingerprint),
                )
            })
            .collect(),
    };
    match mode {
        ScanMode::Block => {
            if let Err(e) = audit
                .record(&record(
                    "secret.block",
                    AuditOutcome::Denied {
                        reason: described.clone(),
                    },
                ))
                .await
            {
                tracing::warn!(error = %e, "auditing a blocked secret failed");
            }
            Err(Error::FailedPrecondition(format!(
                "a credential was found: {described}. Remove it, or if it is a known fake, \
                 add its fingerprint to allow_fingerprints in .rill/secrets.toml"
            )))
        }
        ScanMode::Warn => Ok(audit
            .queue(&record("secret.warn", AuditOutcome::Allowed))?
            .to_vec()),
        ScanMode::Quarantine => {
            let mut out = audit
                .queue(&record("secret.quarantine", AuditOutcome::Allowed))?
                .to_vec();
            out.push(ctx.repo.quarantine_mutation(hash, &described));
            Ok(out)
        }
    }
}

/// The checks every stored version passes.
async fn validate(opened: &Opened, cv: &ChangeVersion) -> Result<()> {
    let ctx = &opened.repo;
    let rules = rill_core::PathRules {
        windows_safe: ctx.config.settings.windows_safe,
    };
    rill_core::validate_version(ctx.repo.objects().as_ref(), cv, rules).await?;
    for parent in &cv.parents {
        if !ctx.repo.has_change_version(parent).await? {
            return Err(Error::precondition(format!(
                "parent {parent} does not exist"
            )));
        }
    }
    if let Some(pred) = &cv.predecessor {
        if !ctx.repo.has_change_version(pred).await? {
            return Err(Error::precondition(format!(
                "predecessor {pred} does not exist"
            )));
        }
    }
    for (path, change) in &cv.file_changes {
        if path.is_root() {
            return Err(Error::invalid("file change at the repository root"));
        }
        if let FileChange::Set {
            content, copy_from, ..
        } = change
        {
            check_content_exists(ctx.repo.objects().as_ref(), content).await?;
            if let Some(source) = copy_from {
                check_copy_source(ctx, source).await?;
            }
        }
    }
    Ok(())
}

/// A copy source must name a file that exists in the version it cites.
async fn check_copy_source(
    ctx: &crate::state::RepoContext,
    source: &rill_types::objects::change::CopySource,
) -> Result<()> {
    let missing = || {
        Error::precondition(format!(
            "copy source {} does not exist in {}",
            source.path, source.version
        ))
    };
    if !ctx.repo.has_change_version(&source.version).await? {
        return Err(missing());
    }
    let root = ctx
        .derive
        .get::<rill_core::RootTree>(&source.version)
        .await?;
    match ctx.trees.lookup(root, &source.path).await? {
        Some(rill_types::EntryKind::File { .. }) => Ok(()),
        _ => Err(missing()),
    }
}
