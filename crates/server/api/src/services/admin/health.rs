//! Health probes of the server's dependencies.

use rill_proto::rill::v1 as pb;
use tonic::Request;

use super::AdminSvc;
use rill_runtime::error::Result;
use rill_runtime::state::AppState;

impl AdminSvc {
    pub(super) async fn health_impl(
        &self,
        _request: Request<pb::HealthRequest>,
    ) -> Result<pb::HealthResponse> {
        let components = health_components(&self.state).await;
        Ok(pb::HealthResponse {
            ok: components.iter().all(|c| c.ok),
            version: env!("CARGO_PKG_VERSION").to_string(),
            components,
            attestation_key: rill_policy::KeyFile::encode_public(&self.state.attestor.public_key()),
        })
    }
}

/// Probe each dependency the server needs to answer requests.
pub async fn health_components(state: &AppState) -> Vec<pb::ComponentHealth> {
    let meta = match state.repos.list().await {
        Ok(repos) => pb::ComponentHealth {
            name: "meta_store".into(),
            ok: true,
            detail: Some(format!("{} repositories", repos.len())),
        },
        Err(e) => pb::ComponentHealth {
            name: "meta_store".into(),
            ok: false,
            detail: Some(e.to_string()),
        },
    };
    let probe = rill_types::ObjectHash::digest(rill_types::ObjectKind::Blob, b"");
    let objects = match state
        .stores
        .objects
        .has(rill_types::ObjectKind::Blob, &probe)
        .await
    {
        Ok(_) => pb::ComponentHealth {
            name: "object_store".into(),
            ok: true,
            detail: None,
        },
        Err(e) => pb::ComponentHealth {
            name: "object_store".into(),
            ok: false,
            detail: Some(e.to_string()),
        },
    };
    let uptime = pb::ComponentHealth {
        name: "server".into(),
        ok: true,
        detail: Some(format!("up {}s", state.started_at.elapsed().as_secs())),
    };
    vec![meta, objects, uptime]
}
