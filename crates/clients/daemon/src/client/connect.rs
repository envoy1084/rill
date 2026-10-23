//! A client of the local daemon, with errors as `rill_sdk::Error` so
//! callers handle server errors relayed by the daemon the same way as
//! errors from the server itself.

use connectrpc::client::{Http2Connection, SharedHttp2Connection};

use crate::config::DaemonDir;
use crate::error::{Error, Result};
use crate::proto as pb;

type Svc = pb::DaemonServiceClient<SharedHttp2Connection>;

/// A connection to a running daemon.
#[derive(Clone)]
pub struct DaemonClient {
    svc: Svc,
    dir: DaemonDir,
}

impl std::fmt::Debug for DaemonClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonClient")
            .field("dir", &self.dir)
            .finish()
    }
}

fn relay(err: connectrpc::ConnectError) -> Error {
    Error::Sdk(rill_sdk::Error::from(err))
}

macro_rules! call {
    ($name:ident, $req:ty, $resp:ty) => {
        #[doc = concat!("`", stringify!($name), "`.")]
        pub async fn $name(&self, request: $req) -> Result<$resp> {
            Ok(self.svc.$name(request).await.map_err(relay)?.into_owned())
        }
    };
}

impl DaemonClient {
    /// Connect to the daemon serving `dir`; fails when none is running.
    pub async fn connect(dir: &DaemonDir) -> Result<Self> {
        let socket = dir.socket();
        if !socket.exists() {
            return Err(Error::Unavailable(format!(
                "no daemon socket at {}",
                socket.display()
            )));
        }
        let authority: http::Uri = "http://rilld"
            .parse()
            .map_err(|e: http::uri::InvalidUri| Error::Invalid(e.to_string()))?;
        let transport = Http2Connection::lazy_unix(socket, authority.clone()).shared(64);
        let config = connectrpc::client::ClientConfig::new(authority);
        let client = DaemonClient {
            svc: Svc::new(transport, config),
            dir: dir.clone(),
        };
        client
            .get_daemon(pb::GetDaemonRequest::default())
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        Ok(client)
    }

    /// The daemon directory.
    pub fn dir(&self) -> &DaemonDir {
        &self.dir
    }

    call!(get_daemon, pb::GetDaemonRequest, pb::GetDaemonResponse);
    call!(
        create_mount,
        pb::CreateMountRequest,
        pb::CreateMountResponse
    );
    call!(get_mount, pb::GetMountRequest, pb::GetMountResponse);
    call!(list_mounts, pb::ListMountsRequest, pb::ListMountsResponse);
    call!(
        delete_mount,
        pb::DeleteMountRequest,
        pb::DeleteMountResponse
    );
    call!(get_status, pb::GetStatusRequest, pb::GetStatusResponse);
    call!(sync_mount, pb::SyncMountRequest, pb::SyncMountResponse);
    call!(
        commit_mount,
        pb::CommitMountRequest,
        pb::CommitMountResponse
    );
    call!(
        update_mount,
        pb::UpdateMountRequest,
        pb::UpdateMountResponse
    );
    call!(
        resolve_paths,
        pb::ResolvePathsRequest,
        pb::ResolvePathsResponse
    );
    call!(prefetch, pb::PrefetchRequest, pb::PrefetchResponse);
    call!(shutdown, pb::ShutdownRequest, pb::ShutdownResponse);
}
