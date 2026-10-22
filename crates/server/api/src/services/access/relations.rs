//! Adding, removing and listing tuples.

use rill_policy::rebac::{dir_object, dir_of_object};
use rill_policy::Relation;
use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

use super::subjects;
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::common::{open_repo, Opened};
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

/// A stored tuple as the API shows it.
pub(super) fn to_proto(object: &str, relation: &str, subject: &str) -> pb::Relation {
    pb::Relation {
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
    parent: &str,
    rel: Option<pb::Relation>,
) -> Result<(Opened, String, Relation, rill_policy::Subject)> {
    let rel = rel.ok_or(rill_proto::ConvertError::missing("relation"))?;
    let opened = open_repo(state, request, parent).await?;
    opened.permit().admin().await?;
    let subject = subjects::stored(&opened, state.dev_mode(), &rel.subject).await?;
    Ok((
        opened,
        object(&rel.path)?,
        relation(&rel.relation)?,
        subject,
    ))
}

pub async fn create(
    state: &AppState,
    request: Call<pb::CreateRelationRequest>,
) -> Result<pb::CreateRelationResponse> {
    let req = request.get_ref();
    let rel = req.relation.clone().into_option();
    let (opened, object, relation, subject) = change(state, &request, &req.parent, rel).await?;
    let version = opened
        .repo
        .authorizer
        .grant(&object, relation, &subject, opened.principal())
        .await?;
    tracing::info!(by = %opened.principal(), repo = %opened.repo.config.name, %object, %relation, %subject, "relation added");
    Ok(pb::CreateRelationResponse {
        repo_uid: opened.repo.id().to_string(),
        relation_version: version,
    })
}

pub async fn delete(
    state: &AppState,
    request: Call<pb::DeleteRelationRequest>,
) -> Result<pb::DeleteRelationResponse> {
    let req = request.get_ref();
    let rel = req.relation.clone().into_option();
    let (opened, object, relation, subject) = change(state, &request, &req.parent, rel).await?;
    let version = opened
        .repo
        .authorizer
        .revoke(&object, relation, &subject, opened.principal())
        .await?;
    tracing::info!(by = %opened.principal(), repo = %opened.repo.config.name, %object, %relation, %subject, "relation removed");
    Ok(pb::DeleteRelationResponse {
        repo_uid: opened.repo.id().to_string(),
        relation_version: version,
    })
}

/// The repository's tuples; owners see them all.
pub async fn list(
    state: &AppState,
    request: Call<pb::ListRelationsRequest>,
) -> Result<pb::ListRelationsResponse> {
    let req = request.get_ref();
    let opened = open_repo(state, &request, &req.parent).await?;
    opened.permit().admin().await?;
    let filter = Filter::parse(
        &req.filter,
        &[
            ("path", Kind::Str),
            ("subject", Kind::Str),
            ("relation", Kind::Str),
        ],
    )?;
    let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
    // Tuples store users and agents by principal id; a filter naming
    // `user:ann` matches the tuples stored for her.
    let mut aliases = std::collections::BTreeMap::new();
    for named in filter.values_of("subject") {
        if let Ok(stored) = subjects::stored(&opened, state.dev_mode(), &named).await {
            aliases.insert(stored.to_string(), named);
        }
    }
    let snapshot = opened
        .repo
        .authorizer
        .snapshot_at_least(opened.ctx.relations_seen_in(opened.repo.id()))
        .await?;
    let relations: Vec<pb::Relation> = snapshot
        .tuples()
        .iter()
        .map(|t| to_proto(&t.object, t.relation.as_str(), &t.subject.to_string()))
        .filter(|r| filter.matches(&Listed(r, &aliases)))
        .collect();
    let (relations, next_page_token) = page.of(relations)?;
    Ok(pb::ListRelationsResponse {
        relations,
        relation_version: snapshot.version(),
        next_page_token,
    })
}

/// A tuple as a list filter sees it, its subject by the name the filter
/// used when it has one.
struct Listed<'a>(
    &'a pb::Relation,
    &'a std::collections::BTreeMap<String, String>,
);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "path" => FieldValue::Str(self.0.path.clone()),
            "subject" => FieldValue::Str(
                self.1
                    .get(&self.0.subject)
                    .unwrap_or(&self.0.subject)
                    .clone(),
            ),
            "relation" => FieldValue::Str(self.0.relation.clone()),
            _ => FieldValue::Missing,
        }
    }
}
