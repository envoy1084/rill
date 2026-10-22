//! Appending imported history to trunk.

use rill_proto::rill::v1 as pb;

use super::AdminSvc;
use crate::services::Call;
use rill_runtime::error::{Error, Result};

impl AdminSvc {
    pub(super) async fn append_trunk_impl(
        &self,
        request: Call<pb::AppendTrunkRequest>,
    ) -> Result<pb::AppendTrunkResponse> {
        let opened = self.open_named(&request, &request.get_ref().repo).await?;
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
        Ok(pb::AppendTrunkResponse { seq: seq.get() })
    }
}
