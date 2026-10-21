//! `/Users` and `/Groups`.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rill_identity::scim::{ListQuery, ScimError};
use serde::Deserialize;
use serde_json::Value;

use super::{scim_error, scim_for, scim_json};
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListParams {
    filter: Option<String>,
    start_index: Option<usize>,
    count: Option<usize>,
}

impl From<ListParams> for ListQuery {
    fn from(p: ListParams) -> Self {
        ListQuery {
            filter: p.filter,
            start_index: p.start_index,
            count: p.count,
        }
    }
}

fn respond(status: StatusCode, result: Result<Value, ScimError>) -> Response {
    match result {
        Ok(body) => scim_json(status, body),
        Err(e) => scim_error(e),
    }
}

fn deleted(result: Result<(), ScimError>) -> Response {
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => scim_error(e),
    }
}

type OrgId = Path<(String, String)>;

pub async fn list_users(
    State(s): State<Arc<AppState>>,
    Path(org): Path<String>,
    Query(q): Query<ListParams>,
) -> Response {
    respond(
        StatusCode::OK,
        scim_for(&s, &org).list_users(&q.into()).await,
    )
}

pub async fn create_user(
    State(s): State<Arc<AppState>>,
    Path(org): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    respond(
        StatusCode::CREATED,
        scim_for(&s, &org).create_user(&body).await,
    )
}

pub async fn get_user(State(s): State<Arc<AppState>>, Path((org, id)): OrgId) -> Response {
    respond(StatusCode::OK, scim_for(&s, &org).get_user(&id).await)
}

pub async fn replace_user(
    State(s): State<Arc<AppState>>,
    Path((org, id)): OrgId,
    Json(body): Json<Value>,
) -> Response {
    respond(
        StatusCode::OK,
        scim_for(&s, &org).replace_user(&id, &body).await,
    )
}

pub async fn patch_user(
    State(s): State<Arc<AppState>>,
    Path((org, id)): OrgId,
    Json(body): Json<Value>,
) -> Response {
    respond(
        StatusCode::OK,
        scim_for(&s, &org).patch_user(&id, &body).await,
    )
}

pub async fn delete_user(State(s): State<Arc<AppState>>, Path((org, id)): OrgId) -> Response {
    deleted(scim_for(&s, &org).delete_user(&id).await)
}

pub async fn list_groups(
    State(s): State<Arc<AppState>>,
    Path(org): Path<String>,
    Query(q): Query<ListParams>,
) -> Response {
    respond(
        StatusCode::OK,
        scim_for(&s, &org).list_groups(&q.into()).await,
    )
}

pub async fn create_group(
    State(s): State<Arc<AppState>>,
    Path(org): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    respond(
        StatusCode::CREATED,
        scim_for(&s, &org).create_group(&body).await,
    )
}

pub async fn get_group(State(s): State<Arc<AppState>>, Path((org, id)): OrgId) -> Response {
    respond(StatusCode::OK, scim_for(&s, &org).get_group(&id).await)
}

pub async fn replace_group(
    State(s): State<Arc<AppState>>,
    Path((org, id)): OrgId,
    Json(body): Json<Value>,
) -> Response {
    respond(
        StatusCode::OK,
        scim_for(&s, &org).replace_group(&id, &body).await,
    )
}

pub async fn patch_group(
    State(s): State<Arc<AppState>>,
    Path((org, id)): OrgId,
    Json(body): Json<Value>,
) -> Response {
    respond(
        StatusCode::OK,
        scim_for(&s, &org).patch_group(&id, &body).await,
    )
}

pub async fn delete_group(State(s): State<Arc<AppState>>, Path((org, id)): OrgId) -> Response {
    deleted(scim_for(&s, &org).delete_group(&id).await)
}
