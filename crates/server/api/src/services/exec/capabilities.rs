//! What this server supports, for clients to negotiate.

use rill_reapi_proto::reapi::capabilities_server::Capabilities;
use rill_reapi_proto::reapi::{
    digest_function, symlink_absolute_path_strategy, ActionCacheUpdateCapabilities,
    CacheCapabilities, ExecutionCapabilities, GetCapabilitiesRequest, ServerCapabilities,
};
use rill_reapi_proto::semver::SemVer;
use tonic::{Request, Response, Status};

use super::access::admit;
use super::service::ExecSvc;

/// The largest batch request or response.
pub const MAX_BATCH_BYTES: i64 = 4 << 20;

fn version(major: i32, minor: i32) -> SemVer {
    SemVer {
        major,
        minor,
        patch: 0,
        prerelease: String::new(),
    }
}

#[tonic::async_trait]
impl Capabilities for ExecSvc {
    async fn get_capabilities(
        &self,
        request: Request<GetCapabilitiesRequest>,
    ) -> Result<Response<ServerCapabilities>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        let blake3 = digest_function::Value::Blake3 as i32;
        Ok(Response::new(ServerCapabilities {
            cache_capabilities: Some(CacheCapabilities {
                digest_functions: vec![blake3],
                action_cache_update_capabilities: Some(ActionCacheUpdateCapabilities {
                    update_enabled: caller.worker,
                }),
                max_batch_total_size_bytes: MAX_BATCH_BYTES,
                symlink_absolute_path_strategy: symlink_absolute_path_strategy::Value::Disallowed
                    as i32,
                ..Default::default()
            }),
            execution_capabilities: Some(ExecutionCapabilities {
                digest_function: blake3,
                exec_enabled: true,
                digest_functions: vec![blake3],
                ..Default::default()
            }),
            low_api_version: Some(version(2, 0)),
            high_api_version: Some(version(2, 3)),
            ..Default::default()
        }))
    }
}
