//! Token signing keys.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;

use crate::auth::{caller, require_instance_admin};
use crate::services::Call;
use rill_runtime::error::Result;
use rill_runtime::state::AppState;

pub async fn rotate(
    state: &AppState,
    request: Call<pb::RotateTokenKeysRequest>,
) -> Result<pb::RotateTokenKeysResponse> {
    let ctx = caller(&request)?;
    require_instance_admin(state, &ctx)?;
    let now = Timestamp::now();
    let keys = state.identity.keys();
    let key_id = keys.rotate(now, &ctx.principal).await?;
    tracing::info!(by = %ctx.principal, key = ?key_id, "token signing key rotated");
    let active_from = now.millis() + keys.policy().activation_delay_ms;
    Ok(pb::RotateTokenKeysResponse {
        key_id,
        active_from: timestamp_to_proto(Timestamp::from_millis(active_from)),
    })
}

pub async fn list(
    state: &AppState,
    request: Call<pb::ListTokenKeysRequest>,
) -> Result<pb::ListTokenKeysResponse> {
    let ctx = caller(&request)?;
    require_instance_admin(state, &ctx)?;
    let keys = state
        .identity
        .store()
        .token_keys()
        .await?
        .into_iter()
        .map(|k| pb::TokenKey {
            id: k.id,
            public_key: hex::encode(&k.public_key),
            backend: k.secret.backend,
            key_ref: k.secret.key_ref,
            created_at: timestamp_to_proto(k.created_at),
            active_from: timestamp_to_proto(k.active_from),
            not_after: k.not_after.map(timestamp_to_proto),
        })
        .collect();
    Ok(pb::ListTokenKeysResponse { keys })
}
