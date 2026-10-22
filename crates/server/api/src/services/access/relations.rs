//! Adding, removing and listing tuples.

use rill_policy::rebac::{dir_object, dir_of_object};
use rill_policy::Relation;
use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

use super::subjects;
use crate::services::common::{open, Opened};
use crate::services::Call;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

pub(super) fn relation(text: &str) -> Result<Relation> {
    text.parse()
        .map_err(|e: rill_policy::Error| Error::invalid(e.to_string()))
}

pub(super) fn object(path: &str) -> Result<String> {
    Ok(dir_object(&RepoPathBuf::new(path)?))
}

/// How requests name the repository: `org/name`, or the bare name in the
/// default org.
pub(super) fn repo_spec(opened: &Opened) -> String {
    let config = &opened.repo.config;
    if config.org == rill_store::DEFAULT_ORG {
        config.name.clone()
    } else {
        format!("{}/{}", config.org, config.name)
    }
}

/// A stored tuple as the API shows it.
pub(super) fn to_proto(
    opened: &Opened,
    object: &str,
    relation: &str,
    subject: &str,
) -> pb::Relation {
    pb::Relation {
        repo: repo_spec(opened),
        path: dir_of_object(object)
            .map(|p| p.to_string())
            .unwrap_or_else(|| object.to_string()),
        relation: relation.to_string(),
        subject: subject.to_string(),
    }
}

/// Resolve and authorize a tuple change: the repository's owners (and
/// org administrators) may change its tuples.
async fn change<T>(
    state: &AppState,
    request: &Call<T>,
    rel: Option<pb::Relation>,
) -> Result<(Opened, String, Relation, rill_policy::Subject)> {
    let rel = rel.ok_or_else(|| Error::invalid("relation is required"))?;
    let opened = open(state, request, &rel.repo).await?;
    opened.permit().admin().await?;
    let subject = subjects::stored(&opened, state.dev_mode(), &rel.subject).await?;
    Ok((
        opened,
        object(&rel.path)?,
        relation(&rel.relation)?,
        subject,
    ))
}

pub async fn add(
    state: &AppState,
    request: Call<pb::AddRelationRequest>,
) -> Result<pb::AddRelationResponse> {
    let rel = request.get_ref().relation.clone();
    let (opened, object, relation, subject) = change(state, &request, rel.into_option()).await?;
    let version = opened
        .repo
        .authorizer
        .grant(&object, relation, &subject, opened.principal())
        .await?;
    tracing::info!(by = %opened.principal(), repo = %opened.repo.config.name, %object, %relation, %subject, "relation added");
    Ok(pb::AddRelationResponse {
        repo_id: opened.repo.id().to_string(),
        relation_version: version,
    })
}

pub async fn remove(
    state: &AppState,
    request: Call<pb::RemoveRelationRequest>,
) -> Result<pb::RemoveRelationResponse> {
    let rel = request.get_ref().relation.clone();
    let (opened, object, relation, subject) = change(state, &request, rel.into_option()).await?;
    let version = opened
        .repo
        .authorizer
        .revoke(&object, relation, &subject, opened.principal())
        .await?;
    tracing::info!(by = %opened.principal(), repo = %opened.repo.config.name, %object, %relation, %subject, "relation removed");
    Ok(pb::RemoveRelationResponse {
        repo_id: opened.repo.id().to_string(),
        relation_version: version,
    })
}

/// Tuples of the repository, by directory, subject or relation; owners
/// see them all.
pub async fn list(
    state: &AppState,
    request: Call<pb::ListRelationsRequest>,
) -> Result<pb::ListRelationsResponse> {
    let req = request.get_ref();
    let opened = open(state, &request, &req.repo).await?;
    opened.permit().admin().await?;
    let object = req.path.as_deref().map(object).transpose()?;
    let subject = match &req.subject {
        Some(s) => Some(
            subjects::stored(&opened, state.dev_mode(), s)
                .await?
                .to_string(),
        ),
        None => None,
    };
    let relation = req.relation.as_deref().map(relation).transpose()?;
    let snapshot = opened
        .repo
        .authorizer
        .snapshot_at_least(opened.ctx.relations_seen_in(opened.repo.id()))
        .await?;
    let relations = snapshot
        .tuples()
        .iter()
        .filter(|t| object.as_ref().is_none_or(|o| *o == t.object))
        .filter(|t| subject.as_ref().is_none_or(|s| *s == t.subject.to_string()))
        .filter(|t| relation.is_none_or(|r| r == t.relation))
        .map(|t| {
            to_proto(
                &opened,
                &t.object,
                t.relation.as_str(),
                &t.subject.to_string(),
            )
        })
        .collect();
    Ok(pb::ListRelationsResponse {
        relations,
        relation_version: snapshot.version(),
    })
}
