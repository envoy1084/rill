//! Parsing capability scopes, actions and policy names, and rendering
//! capabilities back as scopes.

use rill_policy::{Capability, Verb};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

use rill_runtime::error::{Error, Result};
use rill_runtime::state::policy;

/// `repo:<repo>[/<dir>][/*]` to (repo, path).
pub(super) fn parse_scope(scope: &str) -> Result<(String, RepoPathBuf)> {
    let rest = scope
        .strip_prefix("repo:")
        .ok_or_else(|| Error::invalid(format!("scope {scope:?} must start with repo:")))?;
    let rest = rest
        .strip_suffix("/*")
        .or_else(|| rest.strip_suffix('*'))
        .unwrap_or(rest);
    let (repo, dir) = rest.split_once('/').unwrap_or((rest, ""));
    if repo.is_empty() {
        return Err(Error::invalid(format!(
            "scope {scope:?} names no repository"
        )));
    }
    Ok((repo.to_string(), RepoPathBuf::new(dir)?))
}

pub(super) fn parse_action(action: &str) -> Result<Verb> {
    match action {
        "repo.read" | "read" => Ok(Verb::Read),
        "repo.write" | "write" => Ok(Verb::Write),
        "lander.land" | "repo.land" | "land" => Ok(Verb::Land),
        "review.vote" | "repo.review" | "review" => Ok(Verb::Review),
        "repo.admin" | "admin" => Ok(Verb::Admin),
        "exec.execute" | "execute" => Ok(Verb::Execute),
        other => Err(Error::invalid(format!("unknown action {other:?}"))),
    }
}

pub(super) fn capability_protos(repo_name: &str, cap: &Capability) -> Vec<pb::Capability> {
    let mut out = Vec::new();
    for verb in &cap.verbs {
        for path in &cap.paths {
            let scope = if path.is_root() {
                format!("repo:{repo_name}/*")
            } else {
                format!("repo:{repo_name}/{path}/*")
            };
            out.push(pb::Capability {
                action: format!("repo.{verb}"),
                scope,
                expires_at: Some(timestamp_to_proto(cap.expires_at)),
            });
        }
    }
    out
}

/// `<repo>/<policy>`.
pub(super) fn parse_policy_name(name: &str) -> Result<(String, String)> {
    match name.rsplit_once('/') {
        Some((repo, policy)) if !repo.is_empty() => Ok((repo.to_string(), policy.to_string())),
        _ => Err(Error::invalid(format!(
            "policy name {name:?} must be <repo>/{} or <repo>/{}",
            policy::SUBMIT_POLICY,
            policy::RISK_POLICY
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scopes_and_names() {
        assert_eq!(
            parse_scope("repo:main/src/*").unwrap(),
            ("main".into(), RepoPathBuf::new("src").unwrap())
        );
        assert_eq!(parse_scope("repo:main").unwrap().1, RepoPathBuf::root());
        assert_eq!(parse_action("repo.write").unwrap(), Verb::Write);
        assert_eq!(
            parse_policy_name("main/submit").unwrap(),
            ("main".into(), "submit".into())
        );
        assert!(parse_policy_name("submit").is_err());
    }
}
