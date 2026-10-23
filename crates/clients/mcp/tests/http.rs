//! The streamable HTTP transport: a client initializes a session over
//! HTTP and calls tools in it.

mod common;

use common::{Fixture, REPO};
use rill_mcp::ToolContext;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

/// The JSON-RPC message answering `id` in a JSON or event-stream body.
fn answer(body: &str, id: i64) -> Value {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        return v;
    }
    body.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
        .find(|m| m["id"] == id)
        .unwrap_or_else(|| panic!("no answer to {id} in {body}"))
}

#[tokio::test]
async fn a_client_runs_tools_over_streamable_http() {
    let f = Fixture::start().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}{}",
        listener.local_addr().unwrap(),
        rill_mcp::http::PATH
    );
    let shutdown = CancellationToken::new();
    let ctx = ToolContext::new(f.sdk.clone(), Some(REPO.into()));
    tokio::spawn(rill_mcp::http::serve_http(
        ctx,
        listener,
        Vec::new(),
        shutdown.clone(),
    ));

    let http = reqwest::Client::new();
    let post = |session: Option<&str>, body: Value| {
        let mut req = http
            .post(&url)
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18")
            .json(&body);
        if let Some(s) = session {
            req = req.header("mcp-session-id", s);
        }
        req.send()
    };

    let init = post(
        None,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" },
        } }),
    )
    .await
    .unwrap();
    assert!(init.status().is_success(), "{}", init.status());
    let session = init.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_string();
    let init = answer(&init.text().await.unwrap(), 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "rill-mcp");
    post(
        Some(&session),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await
    .unwrap();

    let call = |id: i64, name: &str, args: Value| {
        post(
            Some(&session),
            json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
                    "params": { "name": name, "arguments": args } }),
        )
    };
    let edited = call(
        2,
        "rill_edit",
        json!({ "edits": [{ "op": "set", "path": "a.txt", "content": "over http\n" }] }),
    )
    .await
    .unwrap();
    let edited = answer(&edited.text().await.unwrap(), 2);
    let change = edited["result"]["structuredContent"]["change"].clone();
    assert!(change.as_str().unwrap().starts_with("ch_"), "{edited}");

    // The HTTP session remembers its change like a stdio one.
    let context = call(3, "rill_context", json!({})).await.unwrap();
    let context = answer(&context.text().await.unwrap(), 3);
    assert_eq!(
        context["result"]["structuredContent"]["change"]["id"],
        change
    );
    shutdown.cancel();
}
