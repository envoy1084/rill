//! Landing pipeline for Rill.
//!
//! Landing is optimistic: a [`Lander`] rebases a requested version onto
//! the current trunk head, optionally runs pre-land verification per the
//! [`LandPolicy`], and advances trunk with a compare-and-swap that assigns
//! the next sequence number. Verification is continuous: a
//! [`PostLandVerifier`] follows trunk, bisects red batches to the first
//! red position and, when policy allows, [`revert`]s it.
//!
//! Review-state updates and verification are abstracted behind
//! [`ReviewHooks`] and [`Verifier`] so this crate depends on neither the
//! review crate nor the execution runner.

pub mod config;
pub mod error;
pub mod events;
pub mod ladder;
pub mod land;
pub mod postland;
pub mod queue;
pub mod revert;
pub mod schedule;
pub mod traits;

pub use config::{LandPolicy, VerifyMode, VerifyModes};
pub use error::{Error, Result};
pub use events::LandEvent;
pub use ladder::{MergeRules, MERGE_DRIVERS_EXTRA, MERGE_RULES_PATH};
pub use land::{
    lander_lease, landing_of, list_landings, HeadRules, Lander, Landing, LANDER_LEASE, MAX_BATCH,
};
pub use postland::{PostLandVerifier, Sweep, VerificationRecord, VerificationStatus};
pub use queue::{LandQueue, LandRequest, LandStatus, LandTicket, QueuedRequest};
pub use revert::{auto_revert, build_revert, has_forward_fix, revert, RevertOutcome};
pub use traits::{
    AlwaysGreen, Approval, ConflictResolver, FnVerifier, NoRegenerator, NoopHooks,
    RegenerateRequest, Regenerated, Regenerator, Restacked, ReviewHooks, TargetGraph,
    VerificationOutcome, Verifier, VerifyRequest, VerifyStage,
};
