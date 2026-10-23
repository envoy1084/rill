//! The human output of the everyday commands, as snapshots: what people
//! read is part of the interface too. Values that differ run to run (ids,
//! hashes, times, paths) are redacted; the width is fixed.

mod common;

use std::path::Path;

use common::{approve, Server};
use regex::Regex;

fn redact(text: &str, dir: &str) -> String {
    let mut text = text.replace(dir, "[dir]");
    for (pattern, with) in [
        (r"\bch_[0-9a-f]{32}\b", "ch_[id]"),
        (r"\bch_[0-9a-f]{8}\b", "ch_[short]"),
        (r"\b(ws|op|pr|rp)_[0-9a-f]{32}\b", "${1}_[id]"),
        (r"\b[0-9a-f]{64}\b", "[hash]"),
        (r"\b[0-9a-f]{12}\b", "[hash12]"),
        (r"\b[0-9a-f]{8}\b", "[hash8]"),
        (r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", "[time]"),
        (r"\d+ (second|minute)s? ago", "just now"),
    ] {
        text = Regex::new(pattern)
            .unwrap()
            .replace_all(&text, with)
            .into_owned();
    }
    text
}

async fn shot(server: &Server, cwd: &Path, dir: &str, args: &[&str]) -> String {
    let (code, out) = server.rill(cwd, args).await;
    assert_eq!(code, 0, "{args:?}: {out}");
    redact(&out, dir)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn everyday_commands_read_like_git() {
    std::env::set_var("COLUMNS", "100");
    let server = Server::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(tmp.path()).unwrap();
    let dir = home.display().to_string();
    server.json(&home, &["repo", "create", "look"]).await;
    let co = home.join("look");
    server
        .json(&home, &["checkout", "look", co.to_str().unwrap()])
        .await;
    std::fs::write(
        co.join("main.rs"),
        "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n",
    )
    .unwrap();
    std::fs::write(co.join("README.md"), "# Look\n").unwrap();
    let first = server.json(&co, &["new", "-m", "first change"]).await;
    let first = first["change_id"].as_str().unwrap().to_string();
    server.json(&co, &["submit"]).await;
    approve(&server, &co, &first).await;
    server.json(&co, &["land", "--wait"]).await;
    std::fs::write(
        co.join("main.rs"),
        "fn main() {\n    let x = 2;\n    println!(\"value {}\", x);\n}\n",
    )
    .unwrap();
    std::fs::write(co.join("notes.txt"), "notes\n").unwrap();
    std::fs::remove_file(co.join("README.md")).unwrap();
    server.json(&co, &["new", "-m", "tweak main"]).await;
    std::fs::write(co.join("notes.txt"), "notes\nmore\n").unwrap();

    insta::assert_snapshot!("status", shot(&server, &co, &dir, &["status"]).await);
    insta::assert_snapshot!(
        "status_short",
        shot(&server, &co, &dir, &["status", "-s"]).await
    );
    insta::assert_snapshot!("log", shot(&server, &co, &dir, &["log"]).await);
    insta::assert_snapshot!("diff", shot(&server, &co, &dir, &["diff"]).await);
    insta::assert_snapshot!(
        "diff_stat",
        shot(&server, &co, &dir, &["diff", "--stat"]).await
    );
    insta::assert_snapshot!("show", shot(&server, &co, &dir, &["show"]).await);
    insta::assert_snapshot!(
        "blame",
        shot(&server, &co, &dir, &["blame", "main.rs", "--at", "trunk"]).await
    );
    insta::assert_snapshot!(
        "help_topic",
        shot(&server, &co, &dir, &["help", "stacks"]).await
    );

    let (code, colored) = server.rill_colored(&co, &["diff", "main.rs"]).await;
    assert_eq!(code, 0);
    insta::assert_snapshot!(
        "diff_colored",
        redact(&colored, &dir).replace('\u{1b}', "␛")
    );
}
