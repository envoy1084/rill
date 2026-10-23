//! The `--json` documents the CLI prints, one snapshot per command: the
//! versioned schemas (`rill.cli.v1.*`) scripts and agents rely on. Values
//! that differ run to run (hashes, ids, times, paths) are redacted; a
//! change to a snapshot is a change to a schema.

mod common;

use std::path::Path;

use common::Server;
use regex::Regex;
use serde_json::Value;

/// Values that differ between runs, by pattern.
fn redact(value: &mut Value, dir: &str) {
    let patterns: &[(&str, &str)] = &[
        (
            r"\b(ch|ws|in|op|pr|rp|tk|ss|job)_[0-9a-f]{32}\b",
            "${1}_[id]",
        ),
        (r"\b[0-9a-f]{64}\b", "[hash]"),
        (r"\bmounts/[0-9a-f]{32}\b", "mounts/[id]"),
        (
            r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b",
            "[uuid]",
        ),
        (
            r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})",
            "[time]",
        ),
        (r"127\.0\.0\.1:\d+", "[address]"),
    ];
    let patterns: Vec<(Regex, &str)> = patterns
        .iter()
        .map(|(p, r)| (Regex::new(p).unwrap(), *r))
        .collect();
    walk(value, None, &patterns, dir);
}

fn walk(value: &mut Value, key: Option<&str>, patterns: &[(Regex, &str)], dir: &str) {
    match value {
        Value::String(s) => {
            let mut text = s.replace(dir, "[dir]");
            for (re, with) in patterns {
                text = re.replace_all(&text, *with).into_owned();
            }
            if is_short_hash(&text) {
                text = "[short]".into();
            } else if text.len() >= 12 && text.chars().all(|c| c.is_ascii_digit()) {
                text = "[ticket]".into();
            }
            *s = text;
        }
        Value::Number(n) if key.is_some_and(is_time) || n.as_u64().is_some_and(is_millis) => {
            *value = Value::from(0)
        }
        Value::Array(items) => items.iter_mut().for_each(|v| walk(v, key, patterns, dir)),
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                walk(v, Some(k), patterns, dir);
            }
        }
        _ => {}
    }
}

fn is_time(key: &str) -> bool {
    key == "at" || key.ends_with("_at") || key.ends_with("_ms") || key == "since" || key == "millis"
}

/// A count of milliseconds since the epoch: a time, whatever its key.
fn is_millis(n: u64) -> bool {
    n >= 1_000_000_000_000
}

/// A 10- or 12-character prefix of a hash or id, as tables and short forms
/// print.
fn is_short_hash(text: &str) -> bool {
    matches!(text.len(), 10 | 12) && text.chars().all(|c| c.is_ascii_hexdigit())
}

async fn snapshot(server: &Server, cwd: &Path, dir: &Path, name: &str, args: &[&str]) {
    let mut doc = server.document(cwd, args).await;
    redact(&mut doc, &dir.display().to_string());
    insta::assert_json_snapshot!(name, doc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn json_documents_keep_their_schemas() {
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    // Checkouts report canonical paths (`/private/var/…` on macOS).
    let home = &std::fs::canonicalize(tmp.path()).unwrap();
    let co = home.join("co");
    let co_s = co.to_str().unwrap();
    let s = |name: &'static str, cwd: &Path, args: &'static [&'static str]| {
        let server = &server;
        let cwd = cwd.to_path_buf();
        async move { snapshot(server, &cwd, home, name, args).await }
    };

    s(
        "repo_create",
        home,
        &["repo", "create", "shape", "--description", "schemas"],
    )
    .await;
    s("repo_list", home, &["repo", "list"]).await;
    s("repo_show", home, &["--repo", "shape", "repo", "show"]).await;
    s("org_list", home, &["org", "list"]).await;
    s("org_show", home, &["org", "show", "default"]).await;
    s("whoami", home, &["whoami"]).await;

    snapshot(
        &server,
        home,
        home,
        "checkout",
        &["checkout", "shape", co_s],
    )
    .await;
    std::fs::create_dir_all(co.join("src")).unwrap();
    std::fs::write(co.join("README.md"), "# shape\n").unwrap();
    std::fs::write(co.join("src/lib.rs"), "pub fn one() -> u32 { 1 }\n").unwrap();
    s("status", &co, &["status"]).await;
    s("diff", &co, &["diff"]).await;
    s("new", &co, &["new", "-m", "first change"]).await;
    s(
        "describe",
        &co,
        &["describe", "-m", "first change, described"],
    )
    .await;
    s("submit", &co, &["submit"]).await;
    s("approve", &co, &["approve"]).await;
    let shown = server.json(&co, &["review", "show"]).await;
    let change = shown
        .pointer("/review/change_id")
        .or_else(|| shown.pointer("/change_id"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no change id in {shown}"))
        .to_string();
    server
        .json(&co, &["review", "vote", &change, "Verified", "+1"])
        .await;
    s("review_show", &co, &["review", "show"]).await;
    s("review_list", &co, &["review", "list"]).await;
    s("review_evaluate", &co, &["review", "evaluate"]).await;
    s("review_threads", &co, &["review", "threads"]).await;
    s("land", &co, &["land", "--wait", "--timeout", "30"]).await;
    s("queue", &co, &["queue"]).await;

    s("log", &co, &["log"]).await;
    s("show", home, &["--repo", "shape", "show"]).await;
    s("ls", &co, &["ls"]).await;
    s("cat", &co, &["cat", "README.md"]).await;
    s("blame", &co, &["blame", "src/lib.rs"]).await;
    s("grep", &co, &["grep", "one"]).await;
    s("trunk", &co, &["trunk"]).await;
    s("trunk_health", &co, &["trunk", "health"]).await;
    s("trunk_landings", &co, &["trunk", "landings"]).await;
    s("trunk_verify", &co, &["trunk", "verify", "1"]).await;
    s("incidents", &co, &["incidents"]).await;
    s("ref_list", &co, &["ref", "list"]).await;
    s("release_cut", &co, &["release", "cut", "1.0"]).await;
    s("release_list", &co, &["release", "list"]).await;
    s("sync", &co, &["sync"]).await;
    s("conflicts", &co, &["conflicts"]).await;
    s("op_log", &co, &["op", "log", "-n", "3"]).await;
    s("ws_list", &co, &["ws", "list"]).await;
    s("ws_show", &co, &["ws", "show"]).await;
    s(
        "intent_create",
        &co,
        &["intent", "create", "Shape the API", "--kind", "feature"],
    )
    .await;
    s("intent_list", &co, &["intent", "list"]).await;
    s("access_check", &co, &["access", "check", "reader"]).await;
    s("checks_runs", &co, &["checks", "runs"]).await;
}
