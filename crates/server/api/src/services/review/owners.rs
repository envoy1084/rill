//! Who may cast an `Owner` approval: an owner of every path the change
//! writes, by `.rill/owners.toml` on trunk, or for paths no rule covers, an
//! `owner` of the directory by relation.

use rill_policy::owners::{OwnersFile, OWNERS_PATH};
use rill_policy::{Relation, Verb};
use rill_types::{ChangeId, EntryKind, RepoPathBuf};

use crate::services::common::Opened;
use rill_runtime::error::{Error, Result};
use rill_runtime::writes::write_set;

/// The label whose approvals only owners may give.
pub const OWNER_LABEL: &str = "Owner";

/// The owners file on trunk; absent when trunk has none.
async fn owners_file(opened: &Opened) -> Result<Option<OwnersFile>> {
    let ctx = &opened.repo;
    let Some((_, head)) = ctx.repo.trunk_head().await? else {
        return Ok(None);
    };
    let root = ctx.derive.get::<rill_core::RootTree>(&head).await?;
    let path = RepoPathBuf::new(OWNERS_PATH)?;
    match ctx.trees.lookup(root, &path).await? {
        Some(EntryKind::File { content, .. }) => {
            let bytes = rill_core::read_content(ctx.repo.objects().as_ref(), &content).await?;
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|_| Error::precondition(format!("{OWNERS_PATH} is not UTF-8")))?;
            Ok(Some(OwnersFile::parse(&text)?))
        }
        _ => Ok(None),
    }
}

/// Refuse an `Owner` approval from someone who does not own everything the
/// change's current version writes.
pub async fn check_approval(opened: &Opened, change: &ChangeId) -> Result<()> {
    if opened.dev_mode() {
        return Ok(());
    }
    let ctx = &opened.repo;
    let review = ctx.review().get(change).await?;
    let version = review
        .current_version()
        .ok_or_else(|| Error::precondition(format!("{change} has no version")))?;
    let cv = ctx.derive.change(&version).await?;
    let set = write_set(&ctx.trees, &ctx.derive, &cv).await?;
    let file = owners_file(opened).await?;
    let voter = &opened.ctx.principal;
    for path in &set.writes {
        let owns = match file.as_ref().and_then(|f| f.rule_for(path)) {
            Some(rule) => OwnersFile::is_owner(rule, voter, &opened.ctx.teams),
            None => opened
                .permit()
                .check(Verb::Review, Relation::Owner, path)
                .await
                .is_ok(),
        };
        if !owns {
            return Err(Error::denied(format!(
                "{voter} does not own {path}, so cannot approve as {OWNER_LABEL}"
            )));
        }
    }
    Ok(())
}
