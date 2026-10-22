//! Repository-defined verification: `.rill/verify.toml` compiled to REAPI
//! actions and run by whichever workers serve the scheduler.

mod compile;
pub mod graph;
mod outcome;
mod plan;
mod progress;
mod run;
mod select;
mod submit;

pub use compile::{compile, CompiledAction, ExtraFile};
pub use outcome::{StepResult, StepStatus, VerificationOutcome};
pub use plan::{load_plan, Plan, Step, DEFAULT_STEP_TIMEOUT, MAX_SHARDS, PLAN_PATH};
pub use run::{PlanRunner, Target, AFFECTED_ENV, AFFECTED_FILE};
pub use select::{select, Selection};
