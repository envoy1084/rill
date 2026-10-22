//! Resource names in requests and responses: parsing what a request names
//! into ids, and formatting the names a response returns.

use rill_proto::names::{self, Pattern};
use rill_types::{ChangeId, ChangeVersionHash, OperationId, TrunkSeq, WorkspaceId};

use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// The variables of `name`, which must match `pattern`.
pub fn parse<'a>(pattern: &Pattern, name: &'a str) -> Result<Vec<&'a str>> {
    pattern
        .parse(name)
        .map_err(|e| Error::InvalidName(e.to_string()))
}

/// The repository spec (`org/repo`) a repository-scoped name starts with.
pub fn repo_spec(name: &str) -> Result<String> {
    let (org, repo) = names::repo_of(name).map_err(|e| Error::InvalidName(e.to_string()))?;
    Ok(format!("{org}/{repo}"))
}

/// The org an org-scoped name (`orgs/{org}…`) starts with.
pub fn org_of(name: &str) -> Result<&str> {
    match name.split('/').collect::<Vec<_>>().as_slice() {
        ["orgs", org, ..] if !org.is_empty() => Ok(org),
        _ => Err(Error::InvalidName(format!(
            "{name:?} is not an org-scoped name (orgs/{{org}}/…)"
        ))),
    }
}

/// The org an `orgs/{org}` parent names.
pub fn org(parent: &str) -> Result<String> {
    Ok(parse(&names::ORG, parent)?[0].to_string())
}

/// The org and id of an org resource (`orgs/{org}/secrets/{secret}`).
pub fn org_child(pattern: &Pattern, name: &str) -> Result<(String, String)> {
    let parts = parse(pattern, name)?;
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// An org resource's name.
pub fn org_resource(pattern: &Pattern, org: &str, id: &str) -> String {
    pattern.format(&[org, id])
}

/// `orgs/{org}/repos/{repo}` for a repository.
pub fn repo(ctx: &RepoContext) -> String {
    names::repo(&ctx.config.org, &ctx.config.name)
}

/// `…/versions/{hash}` for a version of a repository.
pub fn version(ctx: &RepoContext, hash: &ChangeVersionHash) -> String {
    names::VERSION.format(&[&ctx.config.org, &ctx.config.name, &hash.to_hex()])
}

/// `…/changes/{change}` for a change of a repository.
pub fn change(ctx: &RepoContext, id: &ChangeId) -> String {
    names::CHANGE.format(&[&ctx.config.org, &ctx.config.name, &id.to_string()])
}

/// `…/trunk/positions/{N}`.
pub fn trunk_position(ctx: &RepoContext, seq: TrunkSeq) -> String {
    names::TRUNK_POSITION.format(&[&ctx.config.org, &ctx.config.name, &seq.get().to_string()])
}

/// `…/refs/{ref}`.
pub fn ref_name(ctx: &RepoContext, short: &str) -> String {
    names::REF.format(&[&ctx.config.org, &ctx.config.name, short])
}

/// `…/workspaces/{workspace}`.
pub fn workspace(ctx: &RepoContext, id: &WorkspaceId) -> String {
    names::WORKSPACE.format(&[&ctx.config.org, &ctx.config.name, &id.to_string()])
}

/// `…/changes/{change}/review`.
pub fn review(ctx: &RepoContext, id: &ChangeId) -> String {
    names::REVIEW.format(&[&ctx.config.org, &ctx.config.name, &id.to_string()])
}

/// `…/changes/{change}/review/threads/{thread}`.
pub fn thread(ctx: &RepoContext, id: &ChangeId, thread: &str) -> String {
    names::THREAD.format(&[&ctx.config.org, &ctx.config.name, &id.to_string(), thread])
}

/// `…/landRequests/{ticket}`.
pub fn land_request(ctx: &RepoContext, ticket: &str) -> String {
    names::LAND_REQUEST.format(&[&ctx.config.org, &ctx.config.name, ticket])
}

/// `…/incidents/{culprit}`.
pub fn incident(ctx: &RepoContext, culprit: TrunkSeq) -> String {
    names::INCIDENT.format(&[
        &ctx.config.org,
        &ctx.config.name,
        &culprit.get().to_string(),
    ])
}

/// `…/intents/{intent}`.
pub fn intent_name(ctx: &RepoContext, id: &rill_types::IntentId) -> String {
    names::INTENT.format(&[&ctx.config.org, &ctx.config.name, &id.to_string()])
}

/// The intent an intent name holds, in `ctx`'s repository.
pub fn intent_in(ctx: &RepoContext, name: &str) -> Result<rill_types::IntentId> {
    let parts = parse(&names::INTENT, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    id_part(parts[2], "intent")
}

/// `…/runs/{run}`.
pub fn run(ctx: &RepoContext, id: &str) -> String {
    names::RUN.format(&[&ctx.config.org, &ctx.config.name, id])
}

/// The run id a run name holds, in `ctx`'s repository.
pub fn run_in(ctx: &RepoContext, name: &str) -> Result<String> {
    let parts = parse(&names::RUN, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    Ok(parts[2].to_string())
}

/// `…/quarantines/{target}`.
pub fn quarantine(ctx: &RepoContext, target: &str) -> String {
    names::QUARANTINE.format(&[&ctx.config.org, &ctx.config.name, target])
}

/// The target a quarantine name holds, in `ctx`'s repository.
pub fn quarantine_in(ctx: &RepoContext, name: &str) -> Result<String> {
    let parts = parse(&names::QUARANTINE, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    Ok(parts[2].to_string())
}

/// `…/operations/{operation}`.
pub fn operation(ctx: &RepoContext, id: &OperationId) -> String {
    names::OPERATION.format(&[&ctx.config.org, &ctx.config.name, &id.to_string()])
}

/// The change a review name is the review of, in `ctx`'s repository.
pub fn review_in(ctx: &RepoContext, name: &str) -> Result<ChangeId> {
    let parts = parse(&names::REVIEW, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    id_part(parts[2], "change")
}

/// The change and thread id a thread name holds, in `ctx`'s repository.
pub fn thread_in(ctx: &RepoContext, name: &str) -> Result<(ChangeId, String)> {
    let parts = parse(&names::THREAD, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    Ok((id_part(parts[2], "change")?, parts[3].to_string()))
}

/// The ticket a land request name holds, in `ctx`'s repository.
pub fn ticket_in(ctx: &RepoContext, name: &str) -> Result<rill_types::LandTicket> {
    let parts = parse(&names::LAND_REQUEST, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    Ok(rill_proto::convert::ticket_from_proto("name", parts[2])?)
}

/// The position a trunk position name holds, in `ctx`'s repository.
pub fn position_in(ctx: &RepoContext, name: &str) -> Result<TrunkSeq> {
    let parts = parse(&names::TRUNK_POSITION, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    parts[2]
        .parse()
        .map(TrunkSeq)
        .map_err(|_| Error::InvalidName(format!("{:?} is not a trunk position", parts[2])))
}

/// The operation id an operation name holds, in `ctx`'s repository.
pub fn operation_in(ctx: &RepoContext, name: &str) -> Result<OperationId> {
    let parts = parse(&names::OPERATION, name)?;
    same_repo(ctx, &format!("{}/{}", parts[0], parts[1]))?;
    id_part(parts[2], "operation")
}

fn id_part<T: std::str::FromStr>(text: &str, kind: &str) -> Result<T> {
    text.parse()
        .map_err(|_| Error::InvalidName(format!("{text:?} is not a {kind} id")))
}

/// The repository spec and version hash a version name holds.
pub fn parse_version(name: &str) -> Result<(String, ChangeVersionHash)> {
    let parts = parse(&names::VERSION, name)?;
    let hash = parts[2]
        .parse()
        .map_err(|_| Error::InvalidName(format!("{:?} is not a version hash", parts[2])))?;
    Ok((format!("{}/{}", parts[0], parts[1]), hash))
}

/// The repository spec and change id a change name holds.
pub fn parse_change(name: &str) -> Result<(String, ChangeId)> {
    let parts = parse(&names::CHANGE, name)?;
    let id = parts[2]
        .parse()
        .map_err(|_| Error::InvalidName(format!("{:?} is not a change id", parts[2])))?;
    Ok((format!("{}/{}", parts[0], parts[1]), id))
}

/// The change id a change name holds, which must be in `ctx`'s repository.
pub fn change_in(ctx: &RepoContext, name: &str) -> Result<ChangeId> {
    let (spec, id) = parse_change(name)?;
    same_repo(ctx, &spec)?;
    Ok(id)
}

/// The hash a version name holds, which must be in `ctx`'s repository.
pub fn version_in(ctx: &RepoContext, name: &str) -> Result<ChangeVersionHash> {
    let (spec, hash) = parse_version(name)?;
    same_repo(ctx, &spec)?;
    Ok(hash)
}

/// Refuse a name from another repository than the request's.
fn same_repo(ctx: &RepoContext, spec: &str) -> Result<()> {
    let (org, repo) = spec.split_once('/').unwrap_or_default();
    let matches =
        org == ctx.config.org && (repo == ctx.config.name || repo == ctx.config.id.to_string());
    if !matches {
        return Err(Error::InvalidName(format!(
            "{spec} is not the repository the request names"
        )));
    }
    Ok(())
}

/// The intent an intent name holds; `None` for an empty name.
pub fn intent(name: &str) -> Result<Option<rill_types::IntentId>> {
    if name.is_empty() {
        return Ok(None);
    }
    let parts = parse(&names::INTENT, name)?;
    Ok(Some(id_part(parts[2], "intent")?))
}

/// The idempotency key a request carries: its `request_id`, a UUID, or a
/// fresh key when it has none.
pub fn request_id(id: &str) -> Result<OperationId> {
    if id.is_empty() {
        return Ok(OperationId::new());
    }
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| {
        Error::Convert(rill_proto::ConvertError::Unknown {
            field: "request_id",
            value: id.to_string(),
        })
    })?;
    Ok(OperationId::from_bytes(*uuid.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_are_uuids_and_empty_means_fresh() {
        let a = request_id("3f2504e0-4f89-11d3-9a0c-0305e82c3301").unwrap();
        assert_eq!(
            a,
            request_id("3F2504E0-4F89-11D3-9A0C-0305E82C3301").unwrap()
        );
        assert_ne!(request_id("").unwrap(), request_id("").unwrap());
        assert!(matches!(request_id("not-a-uuid"), Err(Error::Convert(_))));
    }

    #[test]
    fn names_from_another_kind_are_refused() {
        assert!(matches!(
            parse_version("orgs/o/repos/r/changes/ch_1"),
            Err(Error::InvalidName(_))
        ));
        assert_eq!(repo_spec("orgs/o/repos/r/versions/ab").unwrap(), "o/r");
        assert_eq!(org_of("orgs/acme/repos/r/intents/in_1").unwrap(), "acme");
    }
}
