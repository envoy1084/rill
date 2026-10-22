//! Releasing quarantined versions and rotating data keys.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::IdentityChange;
use rill_types::Timestamp;

use super::ProtectionSvc;
use crate::auth::{caller, require_instance_admin};
use crate::services::common::open_repo;
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

impl ProtectionSvc {
    pub(super) async fn rotate_data_keys_impl(
        &self,
        request: Call<pb::RotateDataKeysRequest>,
    ) -> Result<pb::RotateDataKeysResponse> {
        let ctx = caller(&request)?;
        require_instance_admin(&self.state, &ctx)?;
        let keys = self.state.data_keys.as_ref().ok_or_else(|| {
            Error::FailedPrecondition("objects are not encrypted at rest on this server".into())
        })?;
        let r = request.into_inner();
        if r.rewrap {
            let rewrapped = keys.rewrap().await?;
            tracing::info!(by = %ctx.principal, rewrapped, "data keys rewrapped");
            return Ok(pb::RotateDataKeysResponse {
                key_id: None,
                active_from: 0,
                rewrapped: rewrapped as u32,
            });
        }
        let org = names::org(&r.parent)?;
        let now = Timestamp::now();
        let key_id = keys.rotate(&org).await?;
        tracing::info!(by = %ctx.principal, org, key = ?key_id, "data key rotated");
        Ok(pb::RotateDataKeysResponse {
            key_id,
            active_from: timestamp_to_proto(Timestamp::from_millis(
                now.millis() + rill_identity::keys::DATA_KEY_ACTIVATION_MS,
            )),
            rewrapped: 0,
        })
    }

    pub(super) async fn release_version_impl(
        &self,
        request: Call<pb::ReleaseVersionRequest>,
    ) -> Result<pb::ReleaseVersionResponse> {
        let name = &request.get_ref().version;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().admin().await?;
        let version = names::version_in(&opened.repo, name)?;
        let ctx = &opened.repo;
        if ctx.repo.quarantined(&version).await?.is_none() {
            return Err(Error::not_found(format!("{version} is not quarantined")));
        }
        let log = self.state.stores.identity().log(
            IdentityChange::QuarantineCleared {
                org: ctx.config.org.clone(),
                repo: ctx.config.name.clone(),
                version,
            },
            opened.principal(),
        )?;
        ctx.repo.clear_quarantine(&version, log.to_vec()).await?;
        Ok(pb::ReleaseVersionResponse {})
    }
}
