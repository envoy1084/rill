//! The trunk-red scenarios: each breaks trunk one way and times how the
//! server notices, blames and repairs it.

use std::time::{Duration, Instant};

use rill_proto::rill::v1 as pb;
use rill_types::TrunkSeq;
use serde_json::{json, Value};

use super::slo::Slo;
use super::trunk::{set, Trunk, POLL};

/// How long a scenario waits for trunk to settle before it starts.
const SETTLE: Duration = Duration::from_secs(300);

/// When the server saw a culprit, after it landed.
struct Seen {
    /// Trunk first showed red.
    detected: Instant,
    /// An incident named the culprit.
    attributed: Instant,
    incident: pb::Incident,
}

fn secs(d: Duration) -> f64 {
    (d.as_secs_f64() * 10.0).round() / 10.0
}

/// Wait for an incident blaming `seq`, noting when trunk first went red.
async fn attributed(trunk: &Trunk, seq: TrunkSeq, within: Duration) -> anyhow::Result<Seen> {
    let deadline = Instant::now() + within;
    let mut detected = None;
    while Instant::now() < deadline {
        let (health, open) = trunk.health().await?;
        if detected.is_none() && (health.state != "green" || !open.is_empty()) {
            detected = Some(Instant::now());
        }
        if let Some(incident) = trunk.incident(seq).await? {
            let now = Instant::now();
            return Ok(Seen {
                detected: detected.unwrap_or(now),
                attributed: now,
                incident,
            });
        }
        tokio::time::sleep(POLL).await;
    }
    let all: Vec<_> = trunk
        .incidents()
        .await?
        .into_iter()
        .map(|i| (i.culprit, i.steps, i.resolution))
        .collect();
    anyhow::bail!("no incident blamed {seq} within {within:?}; incidents {all:?}")
}

/// Wait until `seq`'s incident is resolved `how` (or its revert is
/// blocked, for `"blocked"`).
async fn resolved(
    trunk: &Trunk,
    seq: TrunkSeq,
    how: &str,
    within: Duration,
) -> anyhow::Result<(Instant, pb::Incident)> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Some(incident) = trunk.incident(seq).await? {
            let done = match how {
                "blocked" => incident.revert_blocked.is_some(),
                how => incident.resolution.as_deref() == Some(how),
            };
            if done {
                return Ok((Instant::now(), incident));
            }
            if how != "blocked" && incident.resolution.is_some() {
                anyhow::bail!("{seq} was resolved {:?}, not {how}", incident.resolution);
            }
        }
        tokio::time::sleep(POLL).await;
    }
    anyhow::bail!("{seq} was not {how} within {within:?}")
}

/// Whether a landing reverted `seq`.
async fn reverted(trunk: &Trunk, seq: TrunkSeq) -> anyhow::Result<bool> {
    let Some((head, _)) = trunk.repo.trunk_head().await? else {
        return Ok(false);
    };
    Ok(trunk
        .repo
        .trunk_range(seq, head)
        .await?
        .iter()
        .any(|l| l.reverts_seq == Some(seq)))
}

/// The timings of one culprit, checked against the SLO.
fn timings(slo: &Slo, landed: Instant, seen: &Seen, repaired: Option<Instant>) -> Value {
    let detect = seen.detected - landed;
    let attribute = seen.attributed - landed;
    let repair = repaired.map(|at| at - seen.attributed);
    json!({
        "culprit": seen.incident.culprit,
        "steps": seen.incident.steps,
        "detect_secs": secs(detect),
        "attribute_secs": secs(attribute),
        "repair_secs": repair.map(secs),
        "within_slo": detect <= slo.detect
            && attribute <= slo.attribute
            && repair.is_none_or(|r| r <= slo.grace + slo.revert),
    })
}

/// A change breaks check `a`: found, blamed and reverted.
pub async fn breaking_change(trunk: &Trunk, slo: &Slo) -> anyhow::Result<Value> {
    trunk.green(SETTLE).await?;
    let seq = trunk
        .land(vec![set("a/break.txt", "BROKEN\n")], "break a")
        .await?;
    let landed = Instant::now();
    let seen = attributed(trunk, seq, slo.attribute).await?;
    let (repaired, _) = resolved(trunk, seq, "reverted", slo.grace + slo.revert).await?;
    trunk.green(SETTLE).await?;
    Ok(json!({ "culprits": [timings(slo, landed, &seen, Some(repaired))] }))
}

/// Two changes break two checks a second apart, in one batch: each is
/// blamed for its own check and both are reverted.
pub async fn overlapping_culprits(trunk: &Trunk, slo: &Slo) -> anyhow::Result<Value> {
    trunk.green(SETTLE).await?;
    let first = trunk
        .land(vec![set("a/overlap.txt", "BROKEN\n")], "break a")
        .await?;
    let first_landed = Instant::now();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let second = trunk
        .land(vec![set("b/overlap.txt", "BROKEN\n")], "break b")
        .await?;
    let second_landed = Instant::now();
    let (a, b) = tokio::try_join!(
        attributed(trunk, first, slo.attribute),
        attributed(trunk, second, slo.attribute)
    )?;
    anyhow::ensure!(
        a.incident.steps == ["a"] && b.incident.steps == ["b"],
        "blamed for the wrong checks: {:?} and {:?}",
        a.incident.steps,
        b.incident.steps
    );
    let within = slo.grace + slo.revert;
    let ((a_fixed, _), (b_fixed, _)) = tokio::try_join!(
        resolved(trunk, first, "reverted", within),
        resolved(trunk, second, "reverted", within)
    )?;
    trunk.green(SETTLE).await?;
    Ok(json!({ "culprits": [
        timings(slo, first_landed, &a, Some(a_fixed)),
        timings(slo, second_landed, &b, Some(b_fixed)),
    ] }))
}

/// A test known to flake fails on trunk: it is run again, nothing is
/// blamed or reverted, and one that keeps flipping is quarantined.
pub async fn flaky_test(trunk: &Trunk, _slo: &Slo) -> anyhow::Result<Value> {
    trunk.green(SETTLE).await?;
    let within = Duration::from_secs(180);
    // Two passes after a retry make the test known to flake.
    for n in 1..=2 {
        let seq = trunk
            .land(vec![set("flaky/mode", format!("retry-{n}\n"))], "retry")
            .await?;
        trunk.verified(seq, within).await?;
    }
    let mut blamed = Vec::new();
    for n in 1..=3 {
        let seq = trunk
            .land(vec![set("flaky/mode", format!("flaky-{n}\n"))], "flake")
            .await?;
        trunk.verified(seq, within).await?;
        if trunk.incident(seq).await?.is_some() || reverted(trunk, seq).await? {
            blamed.push(seq.get());
        }
    }
    let (health, open) = trunk.health().await?;
    let quarantined: Vec<String> = trunk
        .client
        .checks_svc()
        .list_quarantined(pb::ListQuarantinedRequest {
            repo: trunk.name.clone(),
        })
        .await?
        .into_inner()
        .targets
        .into_iter()
        .map(|t| t.target)
        .collect();
    anyhow::ensure!(
        blamed.is_empty() && open.is_empty() && health.state == "green",
        "a flake was blamed: {blamed:?}, trunk {}",
        health.state
    );
    Ok(json!({ "flaky_landings": 3, "blamed": blamed, "quarantined": quarantined }))
}

/// A later landing edits the culprit's file, so its revert conflicts: the
/// incident records the blocked revert, and the owner's fix closes it.
pub async fn revert_conflict(trunk: &Trunk, slo: &Slo) -> anyhow::Result<Value> {
    trunk.green(SETTLE).await?;
    let seq = trunk
        .land(vec![set("a/conflict.txt", "BROKEN\n")], "break a")
        .await?;
    let landed = Instant::now();
    let seen = attributed(trunk, seq, slo.attribute).await?;
    trunk
        .land(vec![set("a/conflict.txt", "BROKEN, edited\n")], "edit a")
        .await?;
    let (blocked, incident) = resolved(trunk, seq, "blocked", slo.grace + slo.revert).await?;
    trunk
        .land(
            vec![set("a/conflict.txt", "fine\n")],
            &format!("fix a\n\nFixes: {seq}"),
        )
        .await?;
    resolved(trunk, seq, "fixed", slo.revert).await?;
    trunk.green(SETTLE).await?;
    Ok(json!({
        "culprits": [timings(slo, landed, &seen, Some(blocked))],
        "blocked": incident.revert_blocked,
    }))
}

/// The author lands a fix inside the grace period: nothing is reverted.
pub async fn forward_fix(trunk: &Trunk, slo: &Slo) -> anyhow::Result<Value> {
    trunk.green(SETTLE).await?;
    let seq = trunk
        .land(vec![set("a/fix.txt", "BROKEN\n")], "break a")
        .await?;
    let landed = Instant::now();
    let seen = attributed(trunk, seq, slo.attribute).await?;
    trunk
        .land(
            vec![set("a/fix.txt", "fine\n")],
            &format!("fix a\n\nFixes: {seq}"),
        )
        .await?;
    let (fixed, _) = resolved(trunk, seq, "fixed", slo.grace).await?;
    trunk.green(SETTLE).await?;
    // Past the grace period, by when a revert would have been queued.
    let past_grace = (seen.attributed + slo.grace + Duration::from_secs(10))
        .saturating_duration_since(Instant::now());
    tokio::time::sleep(past_grace).await;
    anyhow::ensure!(
        !reverted(trunk, seq).await?,
        "{seq} was reverted after its fix"
    );
    Ok(json!({ "culprits": [timings(slo, landed, &seen, Some(fixed))], "reverted": false }))
}
