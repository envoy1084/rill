//! `rill doctor`: how this host serves checkouts, and what to fix.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rill_daemon::proto as dpb;
use rill_daemon::DaemonClient;
use serde_json::{json, Value};

use crate::context::Ctx;
use crate::output::Out;
use crate::Result;

/// How the server answered.
struct ServerCheck {
    endpoint: String,
    principal: Option<String>,
    latency: Option<Duration>,
    error: Option<String>,
}

/// Run `rill doctor`.
pub async fn run(ctx: &mut Ctx<'_>) -> Result<()> {
    let server = check_server(ctx).await;
    let dir = ctx.daemon_dir()?;
    let daemon = match DaemonClient::connect(&dir).await {
        Ok(d) => {
            let info = d.get_daemon(dpb::GetDaemonRequest::default()).await?;
            let mounts = d
                .list_mounts(dpb::ListMountsRequest::default())
                .await?
                .mounts;
            Some((info, mounts))
        }
        Err(_) => None,
    };
    let value = report_json(&server, &dir, daemon.as_ref());
    ctx.emit(value, |out| render(out, &server, &dir, daemon.as_ref()))
}

async fn check_server(ctx: &mut Ctx<'_>) -> ServerCheck {
    let endpoint = ctx.client_config().map(|c| c.endpoint).unwrap_or_default();
    if endpoint.is_empty() {
        return ServerCheck {
            endpoint,
            principal: None,
            latency: None,
            error: Some("no server configured".into()),
        };
    }
    let started = Instant::now();
    let answer = match ctx.client() {
        Ok(client) => client.admin().whoami().await.map_err(|e| e.into()),
        Err(e) => Err(e),
    };
    match answer {
        Ok(me) => ServerCheck {
            endpoint,
            principal: Some(me.principal.to_string()),
            latency: Some(started.elapsed()),
            error: None,
        },
        Err(e) => ServerCheck {
            endpoint,
            principal: None,
            latency: None,
            error: Some(e.to_string()),
        },
    }
}

fn tier_json(t: Option<&dpb::TierStats>) -> Value {
    let t = t.cloned().unwrap_or_default();
    json!({ "reads": t.reads, "p50_ms": t.p50_ms, "p99_ms": t.p99_ms })
}

fn ago(ms: i64) -> Option<Duration> {
    if ms <= 0 {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some(Duration::from_millis((now - ms).max(0) as u64))
}

fn report_json(
    server: &ServerCheck,
    dir: &rill_daemon::DaemonDir,
    daemon: Option<&(dpb::GetDaemonResponse, Vec<dpb::Mount>)>,
) -> Value {
    let server = json!({
        "endpoint": server.endpoint,
        "reachable": server.error.is_none(),
        "principal": server.principal,
        "latency_ms": server.latency.map(|d| d.as_secs_f64() * 1e3),
        "error": server.error,
    });
    let Some((info, mounts)) = daemon else {
        return json!({
            "server": server,
            "daemon": { "running": false, "dir": dir.root().display().to_string() },
            "checkouts": [],
            "warnings": [],
        });
    };
    let cache = info.cache.clone().into_option().unwrap_or_default();
    json!({
        "server": server,
        "daemon": {
            "running": true,
            "pid": info.pid,
            "version": info.version,
            "dir": info.dir,
        },
        "cache": {
            "objects": cache.objects,
            "bytes": cache.bytes,
            "budget_bytes": cache.budget_bytes,
            "free_bytes": info.cache_free_bytes,
            "evictions": cache.evictions,
        },
        "tiers": {
            "local": tier_json(info.local.as_option()),
            "regional": tier_json(info.regional.as_option()),
            "origin": tier_json(info.origin.as_option()),
            "regional_fallbacks": info.regional_fallbacks,
        },
        "checkouts": mounts.iter().map(|m| json!({
            "path": m.path,
            "repo": m.repo,
            "view": m.view,
            "base": m.base,
            "pending": m.pending,
            "last_sync_ms": (m.last_sync_ms > 0).then_some(m.last_sync_ms),
            "sync_error": (!m.sync_error.is_empty()).then_some(&m.sync_error),
        })).collect::<Vec<_>>(),
        "warnings": info.warnings.iter().map(|w| json!({
            "subject": w.subject, "message": w.message, "fix": w.fix,
        })).collect::<Vec<_>>(),
    })
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1u64 << 30) as f64)
}

fn render(
    out: &mut Out<'_>,
    server: &ServerCheck,
    dir: &rill_daemon::DaemonDir,
    daemon: Option<&(dpb::GetDaemonResponse, Vec<dpb::Mount>)>,
) -> std::io::Result<()> {
    let label = |out: &Out<'_>, s: &str| out.bold(&format!("{s:<10}"));
    match (&server.principal, &server.error) {
        (Some(who), _) => out.line(format!(
            "{} {} as {who} ({:.0} ms)",
            label(out, "server"),
            server.endpoint,
            server.latency.unwrap_or_default().as_secs_f64() * 1e3
        ))?,
        (None, Some(e)) => out.line(format!(
            "{} {} {}",
            label(out, "server"),
            server.endpoint,
            out.bad(&format!("unreachable: {e}"))
        ))?,
        (None, None) => {}
    }
    let Some((info, mounts)) = daemon else {
        out.line(format!(
            "{} {} {}",
            label(out, "daemon"),
            out.warn("not running"),
            out.dim(&format!(
                "({}; any checkout command starts it)",
                dir.root().display()
            ))
        ))?;
        return Ok(());
    };
    out.line(format!(
        "{} pid {}, v{}, {}",
        label(out, "daemon"),
        info.pid,
        info.version,
        out.dim(&info.dir)
    ))?;
    let cache = info.cache.clone().into_option().unwrap_or_default();
    let local = info.local.clone().into_option().unwrap_or_default();
    let regional = info.regional.clone().into_option().unwrap_or_default();
    let origin = info.origin.clone().into_option().unwrap_or_default();
    let total = (local.reads + regional.reads + origin.reads).max(1) as f64;
    let share = |n: u64| format!("{:.1}%", n as f64 * 100.0 / total);
    out.line(format!(
        "{} {} of {} ({} free), {} objects",
        label(out, "cache"),
        gib(cache.bytes),
        gib(cache.budget_bytes),
        gib(info.cache_free_bytes),
        cache.objects
    ))?;
    out.line(format!(
        "{} local {} (p50 {:.1} ms) · regional {} (p50 {:.1} ms) · origin {} (p50 {:.1} ms)",
        label(out, "reads"),
        share(local.reads),
        local.p50_ms,
        share(regional.reads),
        regional.p50_ms,
        share(origin.reads),
        origin.p50_ms
    ))?;
    if mounts.is_empty() {
        out.line(format!("{} {}", label(out, "checkouts"), out.dim("none")))?;
    } else {
        out.line(label(out, "checkouts"))?;
        for m in mounts {
            let sync = match (m.sync_error.is_empty(), ago(m.last_sync_ms)) {
                (false, _) => out.bad(&format!("sync error: {}", m.sync_error)),
                (true, Some(d)) => format!("synced {}s ago", d.as_secs()),
                (true, None) => out.dim("not synced yet"),
            };
            out.line(format!(
                "  {}  {:<6} base {}  pending {}  {sync}",
                m.path, m.view, m.base, m.pending
            ))?;
        }
    }
    if info.warnings.is_empty() {
        out.line(format!("{} {}", label(out, "warnings"), out.good("none")))?;
    } else {
        out.line(label(out, "warnings"))?;
        for w in &info.warnings {
            out.line(format!("  {}: {}", w.subject, out.warn(&w.message)))?;
            out.line(format!("    {} {}", out.dim("fix:"), w.fix))?;
        }
    }
    Ok(())
}
