//! The overlay: a mount's local edits as the latest operation per path,
//! persisted before they are applied, and the status rules over it.

mod record;
mod state;
mod status;

pub use record::{OverlayEntry, OverlayOp};
pub use state::Overlay;
pub use status::{classify, BaseFile, ChangeKind, LocalChange};
