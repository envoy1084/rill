//! JSON encoding of SDK and `rill-types` values.
//!
//! This is the one machine-readable rendering of Rill data: MCP tool
//! results and `rill --json` both use it, so agents see the same shapes
//! whichever surface they drive. Hashes and ids are hex strings with their
//! display prefixes, timestamps are RFC 3339, enums are `snake_case`
//! strings, and byte content is UTF-8 when valid and hex otherwise.

mod admin;
mod checks;
mod diff;
mod history;
mod intent;
mod lander;
mod primitives;
mod review;
mod search;
mod workspace;

pub use admin::{capability, health, issued_token, policy, relation, whoami};
pub use checks::{run_status, run_step, step_failed, step_state, verify_run};
pub use diff::{diff_result, file_diff, hunk};
pub use history::{
    blame_line, change_info, change_version, dir_entry, file_change, land_event, log_page,
    path_history_entry, path_history_page, ref_event, ref_info, repo_info, version_result,
};
pub use intent::{intent, scored_intent};
pub use lander::{
    bisect_result, landing, revert_result, trunk_status, verification_record, verification_report,
};
pub use primitives::{bytes, enum_name, hash, opt, page, principal, timestamp};
pub use review::{
    land_progress, land_ticket, landed_change, new_thread, review_event, review_info, review_state,
    submit_decision, sync_result, thread,
};
pub use search::{grep_hit, grep_result};
pub use workspace::{operation, sparse_profile, workspace, workspace_op};
