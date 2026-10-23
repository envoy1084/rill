//! The view of a lazy or snapshot mount, independent of the kernel
//! interface that serves it: what each path is, directory listings, and
//! the operations (open, read, write, rename, …) as async functions.
//! `readdir` and `stat` read trees only; contents are fetched into the
//! object cache on first open. Front ends: FUSE on Linux (`fuse/`), an
//! NFSv3 loopback server on macOS (`nfs/`).

mod attrs;
mod handles;
mod inodes;
mod node;
mod ops;
mod view;

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use rill_types::RepoPathBuf;

pub(crate) use attrs::{Attr, AttrKind};
#[cfg(target_os = "linux")]
pub(crate) use handles::Handle;
#[cfg(target_os = "macos")]
pub(crate) use inodes::ROOT;
pub(crate) use view::View;

use crate::eager::MARKER;
use crate::error::Result;
use crate::mount::Mount;

/// Something that keeps a view mounted until it is dropped.
pub type Session = Box<dyn Send>;

/// Spotlight skips a volume with this file at its root.
const NEVER_INDEX: &str = ".metadata_never_index";

/// Mount `mount`'s view at its directory with the platform's front end,
/// showing `marker` as the checkout marker.
pub(crate) async fn start(mount: &Arc<Mount>, marker: Bytes) -> Result<Session> {
    let mut virtuals = BTreeMap::new();
    virtuals.insert(RepoPathBuf::new(MARKER)?, marker);
    if cfg!(target_os = "macos") {
        virtuals.insert(RepoPathBuf::new(NEVER_INDEX)?, Bytes::new());
    }
    let view = Arc::new(View::new(
        mount.clone(),
        tokio::runtime::Handle::current(),
        virtuals,
    ));
    serve(mount, view).await
}

#[cfg(target_os = "linux")]
async fn serve(mount: &Arc<Mount>, view: Arc<View>) -> Result<Session> {
    crate::fuse::start(mount, view)
}

#[cfg(target_os = "macos")]
async fn serve(mount: &Arc<Mount>, view: Arc<View>) -> Result<Session> {
    crate::nfs::start(mount, view).await
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn serve(_mount: &Arc<Mount>, _view: Arc<View>) -> Result<Session> {
    Err(crate::error::Error::Unsupported(
        "lazy and snapshot mounts need Linux or macOS; use an eager checkout".into(),
    ))
}
