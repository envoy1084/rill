//! Driving one seeded run from setup to settled state.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rill_core::ChangeBuilder;
use rill_lander::{LandRequest, LandStatus};
use rill_store::repo::LEASE_TTL_MS;
use rill_types::testing::{set_clock, set_id_seed};
use rill_types::{Principal, RiskClass, Timestamp};

use crate::actors::{Action, Agent, Ctx, Outcome, FILES, FILE_LINES, HOT_FILE};
use crate::faults::FaultPlan;
use crate::oracle::{Oracle, Violation};
use crate::rng::SimRng;
use crate::scenario::Scenario;
use crate::world::World;

/// Simulated wall-clock time at the start of every run.
pub const START_MILLIS: i64 = 1_700_000_000_000;

/// How far past the last step the clock jumps to settle jobs: beyond
/// every lease and backoff.
const SETTLE_MILLIS: i64 = 3_600_000;

/// Sweeps allowed for the queue to drain once faults stop.
const SETTLE_SWEEPS: usize = 20;

/// What one run did and which invariants it broke.
#[derive(Debug, Clone)]
pub struct Report {
    pub scenario: &'static str,
    pub seed: u64,
    pub steps: usize,
    pub faults_injected: u64,
    pub restarts: usize,
    pub trunk_length: usize,
    /// Derive jobs whose worker died holding them.
    pub killed_jobs: usize,
    /// Landings a replaced lander tried with its stale lease.
    pub fenced_attempts: usize,
    pub violations: Vec<Violation>,
    /// Hash of every action and outcome; equal seeds give equal digests.
    pub digest: u64,
}

/// Run `scenario` with `seed` on a paused-clock, single-threaded runtime.
pub fn run(seed: u64, scenario: &Scenario) -> Report {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("a current-thread runtime always builds")
        .block_on(run_async(seed, scenario))
}

async fn run_async(seed: u64, scenario: &Scenario) -> Report {
    let mut rng = SimRng::new(seed);
    let plan = Arc::new(FaultPlan::new(
        rng.next_u64(),
        scenario.fail_rate,
        scenario.lose_ack_rate,
        scenario.corrupt_rate,
    ));
    set_clock(Some(START_MILLIS));
    set_id_seed(Some(rng.next_u64()));
    let mut world = World::new(plan.clone()).await;
    seed_trunk(&world).await;

    let mut agents: Vec<Agent> = (0..scenario.agents)
        .map(|i| Agent {
            principal: Principal::agent(format!("agent-{i}")),
            changes: Vec::new(),
        })
        .collect();
    let reviewers = [Principal::human("rev-0"), Principal::human("rev-1")];
    let mut oracle = Oracle::default();
    let mut trace = DefaultHasher::new();
    let mut restarts = 0;

    plan.set_enabled(true);
    for step in 0..scenario.steps {
        if rng.chance(scenario.restart_rate) {
            world.restart().await;
            restarts += 1;
            ("restart", step).hash(&mut trace);
            continue;
        }
        let who = rng.below(agents.len());
        let action = Action::draw(&mut rng);
        let mut ctx = Ctx {
            world: &world,
            oracle: &mut oracle,
            rng: &mut rng,
            reviewers: &reviewers,
            hot_file_share: scenario.hot_file_share,
            stale_base_share: scenario.stale_base_share,
            step,
        };
        set_clock(Some(START_MILLIS + step as i64 * 1_000));
        let outcome = ctx.act(&mut agents[who], action).await;
        (step, who, format!("{action:?}"), outcome_code(outcome)).hash(&mut trace);
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
    }

    plan.set_enabled(false);
    // Past any lease a restarted lander left behind, so the current one
    // can take over and serve every stored request.
    set_clock(Some(
        START_MILLIS + scenario.steps as i64 * 1_000 + LEASE_TTL_MS + 1_000,
    ));
    for _ in 0..SETTLE_SWEEPS {
        if world.lander.queue().is_empty().await.unwrap_or(false) {
            break;
        }
        let _ = world.lander.run_once().await;
    }
    oracle.observe_trunk(&world.repo).await;
    oracle.check_durability(&world.repo).await;
    oracle.check_settled(&world.repo).await;
    oracle.check_event_log(&world.repo).await;
    let settle_at = START_MILLIS + scenario.steps as i64 * 1_000 + SETTLE_MILLIS;
    oracle
        .check_jobs(&world, settle_at, settle_at + SETTLE_MILLIS)
        .await;
    oracle.trunk_len().hash(&mut trace);

    set_clock(None);
    set_id_seed(None);
    Report {
        scenario: scenario.name,
        seed,
        steps: scenario.steps,
        faults_injected: plan.injected(),
        restarts,
        trunk_length: oracle.trunk_len(),
        killed_jobs: oracle.killed_jobs.len(),
        fenced_attempts: oracle.fenced_attempts,
        violations: oracle.violations,
        digest: trace.finish(),
    }
}

fn outcome_code(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Done => 0,
        Outcome::Skipped => 1,
        Outcome::Failed => 2,
    }
}

/// Land the initial files as trunk@1, before faults are enabled.
async fn seed_trunk(world: &World) {
    let body: String = (0..FILE_LINES).map(|i| format!("line {i}\n")).collect();
    let mut builder = ChangeBuilder::new(Principal::human("setup"))
        .message("seed")
        .created_at(Timestamp::from_millis(START_MILLIS));
    for path in FILES.iter().chain([&HOT_FILE]) {
        builder = builder
            .file(path, body.clone())
            .expect("fixed paths are valid");
    }
    let (hash, cv) = builder
        .commit(&world.repo)
        .await
        .expect("setup runs without faults");
    let request = LandRequest::new(
        cv.change_id,
        hash,
        Principal::human("setup"),
        RiskClass::Low,
    );
    let status = world
        .lander
        .land_now(request)
        .await
        .expect("setup runs without faults");
    assert!(
        matches!(status, LandStatus::Landed { .. }),
        "seed landing: {status:?}"
    );
}
