//! Run simulation seeds from the command line, for long local runs and for
//! replaying a failing seed.

use std::collections::BTreeMap;
use std::process::ExitCode;

use clap::Parser;
use rill_sim::{known, run, Scenario};

#[derive(Parser)]
#[command(about = "Deterministic simulation of Rill's review and landing workflow")]
struct Args {
    /// clean, faulty or contended.
    #[arg(long, default_value = "faulty")]
    scenario: String,
    /// Number of seeds to run, starting at --from.
    #[arg(long, default_value_t = 1000)]
    seeds: u64,
    /// First seed.
    #[arg(long, default_value_t = 0)]
    from: u64,
    /// Replay one seed and print every violation.
    #[arg(long)]
    seed: Option<u64>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let Some(scenario) = Scenario::by_name(&args.scenario) else {
        eprintln!("unknown scenario {:?}", args.scenario);
        return ExitCode::from(2);
    };
    if let Some(seed) = args.seed {
        let report = run(seed, &scenario);
        println!(
            "seed {seed}: trunk {} positions, {} faults, {} restarts, digest {:016x}",
            report.trunk_length, report.faults_injected, report.restarts, report.digest
        );
        for v in &report.violations {
            println!("  {v}");
        }
        return if report.violations.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let mut counts: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let (mut unknown, mut killed, mut fenced, mut cut, mut rival, mut kills) = (0, 0, 0, 0, 0, 0);
    for seed in args.from..args.from + args.seeds {
        let report = run(seed, &scenario);
        killed += report.killed_jobs;
        fenced += report.fenced_attempts;
        cut += report.partitions;
        rival += report.rival_landings;
        kills += report.kills;
        for v in report.violations {
            if known(v.invariant).is_none() {
                unknown += 1;
            }
            counts
                .entry(format!("{:?}", v.invariant))
                .or_insert((0, seed))
                .0 += 1;
        }
    }
    println!(
        "{} seeds of {}: {killed} jobs rerun after their worker died, {fenced} landings by replaced landers refused, \
         {cut} partitions, {rival} requests landed by the rival process, {kills} agents killed",
        args.seeds, scenario.name
    );
    for (invariant, (count, first_seed)) in &counts {
        println!("  {invariant}: {count} (first at seed {first_seed})");
    }
    if unknown == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
