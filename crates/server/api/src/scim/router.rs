//! The SCIM routes and their body limit.

use std::sync::Arc;

use axum::middleware;
use axum::routing::get;
use axum::Router;
use rill_runtime::state::AppState;

use super::{credential, discovery, resources};

/// Largest SCIM request body.
pub const MAX_SCIM_BODY: usize = 4 << 20;

/// The SCIM routes.
pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/scim/v2/{org}/ServiceProviderConfig",
            get(discovery::service_provider_config),
        )
        .route(
            "/scim/v2/{org}/ResourceTypes",
            get(discovery::resource_types),
        )
        .route("/scim/v2/{org}/Schemas", get(discovery::schemas))
        .route(
            "/scim/v2/{org}/Users",
            get(resources::list_users).post(resources::create_user),
        )
        .route(
            "/scim/v2/{org}/Users/{id}",
            get(resources::get_user)
                .put(resources::replace_user)
                .patch(resources::patch_user)
                .delete(resources::delete_user),
        )
        .route(
            "/scim/v2/{org}/Groups",
            get(resources::list_groups).post(resources::create_group),
        )
        .route(
            "/scim/v2/{org}/Groups/{id}",
            get(resources::get_group)
                .put(resources::replace_group)
                .patch(resources::patch_group)
                .delete(resources::delete_group),
        )
        // SCIM bodies are single users and groups; a group's member list
        // is the largest.
        .layer(axum::extract::DefaultBodyLimit::max(MAX_SCIM_BODY))
        .layer(middleware::from_fn_with_state(
            state,
            credential::require_scim_credential,
        ))
}
