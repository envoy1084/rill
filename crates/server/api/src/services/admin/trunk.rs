//! Appending imported history to trunk.

use rill_proto::rill::v1 as pb;

use super::AdminSvc;
use crate::services::common::open_repo;
use crate::services::Call;
use rill_runtime::error::{Error, Result};

impl AdminSvc {
    /// Append an imported version to trunk: the one way, besides landing,
    /// for history to reach trunk, so importers can bring existing history
    /// over.
    pub(super) async fn import_trunk_position_impl(
        &self,
        request: Call<pb::ImportTrunkPositionRequest>,
    ) -> Result<pb::ImportTrunkPositionResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().admin().await?;
        let req = request.get_ref();
        let expected = rill_proto::convert::opt_hash_from_proto("expected", req.expected.as_ref())?;
        let version = rill_proto::convert::hash_from_proto("version", &req.version)?;
        if let Some(why) =
            rill_runtime::quarantine::held_back(&opened.repo.repo, &opened.repo.graph, &version)
                .await?
        {
            return Err(Error::FailedPrecondition(format!(
                "{version} is quarantined by secret scanning ({why}); an owner must clear it"
            )));
        }
        let seq = opened
            .repo
            .repo
            .land_on_trunk(expected, version)
            .await
            .map_err(|e| {
                if e.is_conflict() {
                    Error::FailedPrecondition("trunk is not at the expected head".into())
                } else {
                    e.into()
                }
            })?;
        Ok(pb::ImportTrunkPositionResponse {
            position: seq.get(),
        })
    }
}
