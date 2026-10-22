//! Subjects as clients name them (`user:ann`) and as tuples store them
//! (`principal:pr_…`), and the principal a question is about.

use rill_policy::Subject;
use rill_types::Principal;

use crate::error::{Error, Result};
use crate::services::common::Opened;

/// What a tuple stores for `text`. Users and agents must exist in the
/// repository's org (except in dev mode, which has no directory).
pub async fn stored(opened: &Opened, dev_mode: bool, text: &str) -> Result<Subject> {
    match named(opened, dev_mode, text).await? {
        Some((principal, _)) => Ok(Subject::principal(&principal)),
        None => text
            .parse()
            .map_err(|e: rill_policy::Error| Error::invalid(e.to_string())),
    }
}

/// The principal and teams a check is about: the caller when `subject` is
/// absent, otherwise the named user or agent, which needs owner.
pub async fn about(
    opened: &Opened,
    dev_mode: bool,
    subject: Option<&str>,
) -> Result<(Principal, Vec<String>)> {
    let Some(text) = subject.filter(|s| !s.is_empty()) else {
        return Ok((opened.ctx.principal.clone(), opened.ctx.teams.clone()));
    };
    let named = named(opened, dev_mode, text)
        .await?
        .ok_or_else(|| Error::invalid("subject must be user:<name> or agent:<name>"))?;
    if named.0.id != opened.ctx.principal.id {
        opened.permit().admin().await?;
    }
    Ok(named)
}

async fn named(
    opened: &Opened,
    dev_mode: bool,
    text: &str,
) -> Result<Option<(Principal, Vec<String>)>> {
    let identity = &opened.repo.identity;
    let org = opened.repo.config.org.as_str();
    let missing = |what: &str, name: &str| Error::invalid(format!("no {what} {name:?} in {org}"));
    if let Some(name) = text.strip_prefix("user:") {
        return match identity.user(org, name).await? {
            Some(user) => Ok(Some((user.principal.clone(), user.teams.clone()))),
            None if dev_mode => Ok(Some((Principal::human(name), vec![]))),
            None => Err(missing("user", name)),
        };
    }
    if let Some(name) = text.strip_prefix("agent:") {
        return match identity.agent(org, name).await? {
            Some(agent) => Ok(Some((agent.principal.clone(), vec![]))),
            None if dev_mode => Ok(Some((Principal::agent(name), vec![]))),
            None => Err(missing("agent", name)),
        };
    }
    Ok(None)
}
