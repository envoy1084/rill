//! Questions about the tuples: may someone do this, who may, and where.

use rill_policy::{Relation, Subject};
use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

use super::relations::{object, relation, to_proto};
use super::subjects;
use crate::services::common::{open_repo, Opened};
use crate::services::Call;
use rill_runtime::error::Result;
use rill_runtime::state::AppState;

async fn snapshot(opened: &Opened) -> Result<std::sync::Arc<rill_policy::rebac::Snapshot>> {
    Ok(opened
        .repo
        .authorizer
        .snapshot_at_least(opened.ctx.relations_seen_in(opened.repo.id()))
        .await?)
}

/// Whether the subject holds the relation, and why. Asking about oneself
/// needs nothing beyond being able to name the repository.
pub async fn check(
    state: &AppState,
    request: Call<pb::CheckAccessRequest>,
) -> Result<pb::CheckAccessResponse> {
    let req = request.get_ref();
    let opened = open_repo(state, &request, &req.parent).await?;
    let (principal, teams) =
        subjects::about(&opened, state.dev_mode(), req.subject.as_deref()).await?;
    let relation = relation(&req.relation)?;
    let object = object(&req.path)?;
    let snapshot = snapshot(&opened).await?;
    let because: Vec<pb::Relation> = snapshot
        .grants_for(relation, &object)
        .into_iter()
        .filter(|t| t.subject.matches(&principal, &teams))
        .map(|t| to_proto(&t.object, t.relation.as_str(), &t.subject.to_string()))
        .collect();
    let org_admin = because.is_empty() && principal.id == opened.ctx.principal.id && {
        crate::auth::administers_org(
            &opened.repo.identity,
            state.dev_mode(),
            &opened.ctx,
            &opened.repo.config.org,
        )
        .await?
    };
    Ok(pb::CheckAccessResponse {
        allowed: !because.is_empty() || org_admin,
        because,
        org_admin,
        relation_version: snapshot.version(),
    })
}

/// Every tuple granting the relation on the path, with the members of
/// team subjects. Owners only.
pub async fn expand(
    state: &AppState,
    request: Call<pb::ExpandAccessRequest>,
) -> Result<pb::ExpandAccessResponse> {
    let req = request.get_ref();
    let opened = open_repo(state, &request, &req.parent).await?;
    opened.permit().admin().await?;
    let relation = relation(&req.relation)?;
    let snapshot = snapshot(&opened).await?;
    let grants = snapshot.grants_for(relation, &object(&req.path)?);
    let users = if grants.iter().any(|t| matches!(t.subject, Subject::Team(_))) {
        opened
            .repo
            .identity
            .store()
            .users(&opened.repo.config.org)
            .await?
    } else {
        Vec::new()
    };
    let tuples = grants
        .into_iter()
        .map(|t| pb::ExpandedTuple {
            tuple: (to_proto(&t.object, t.relation.as_str(), &t.subject.to_string())).into(),
            members: match &t.subject {
                Subject::Team(team) => users
                    .iter()
                    .filter(|u| u.is_active() && u.teams.contains(team))
                    .map(|u| pb::Principal::from(u.principal.clone()))
                    .collect(),
                _ => Vec::new(),
            },
        })
        .collect();
    Ok(pb::ExpandAccessResponse {
        tuples,
        relation_version: snapshot.version(),
    })
}

/// The directories where the subject holds the relation.
pub async fn lookup_paths(
    state: &AppState,
    request: Call<pb::LookupPathsRequest>,
) -> Result<pb::LookupPathsResponse> {
    let req = request.get_ref();
    let opened = open_repo(state, &request, &req.parent).await?;
    let (principal, teams) =
        subjects::about(&opened, state.dev_mode(), req.subject.as_deref()).await?;
    let relation: Relation = relation(&req.relation)?;
    let snapshot = snapshot(&opened).await?;
    let mut paths: Vec<RepoPathBuf> = snapshot.granted_dirs(&principal, &teams, relation);
    if paths.iter().any(|p| p.is_root()) {
        paths = vec![RepoPathBuf::root()];
    } else {
        let all = paths.clone();
        paths.retain(|p| !all.iter().any(|q| q != p && p.starts_with(q)));
    }
    Ok(pb::LookupPathsResponse {
        paths: paths.iter().map(|p| p.to_string()).collect(),
        relation_version: snapshot.version(),
    })
}
