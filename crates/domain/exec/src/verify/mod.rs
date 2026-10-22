//! Repository-defined verification: `.rill/verify.toml` compiled to REAPI
//! actions and run by whichever workers serve the scheduler.

mod compile;
mod outcome;
mod plan;
mod run;
mod select;

pub use compile::{compile, CompiledAction};
pub use outcome::{StepResult, StepStatus, VerificationOutcome};
pub use plan::{load_plan, Plan, Step, DEFAULT_STEP_TIMEOUT, MAX_SHARDS, PLAN_PATH};
pub use run::{PlanRunner, Target};
pub use select::{select, Selection};
