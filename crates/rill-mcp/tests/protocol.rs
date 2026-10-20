//! Protocol-level behaviour of the MCP server against a mock repository.

mod common;

use common::{sample_info, Harness, MockRepo};
use rill_mcp::protocol::{INVALID_PARAMS, METHOD_NOT_FOUND, PARSE_ERROR, PROTOCOL_VERSION};
use serde_json::{json, Value};
use tokio::io::BufReader;

async fn harness() -> Harness {
    let repo = MockRepo::default();
    repo.set_tree(
        "trunk",
        vec![
            ("README.md", b"# hello\nfn main\n"),
            ("src/lib.rs", b"pub fn main() {}\n"),
            ("src/big.bin", &[0xffu8; 300][..]),
        ],
    );
    repo.log.lock().unwrap().push(sample_info("a", Some(3)));
    Harness::start(repo).await
}

/// Drive the server through its stdio loop with in-memory streams.
async fn roundtrip(h: &Harness, lines: &str) -> Vec<Value> {
    let mut out = Vec::new();
    h.server
        .serve(BufReader::new(lines.as_bytes()), &mut out)
        .await
        .unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
async fn initialize_handshake_roundtrip() {
    let h = harness().await;
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        "\n",
    );
    let out = roundtrip(&h, input).await;
    assert_eq!(out.len(), 2, "notification must not be answered: {out:?}");
    assert_eq!(out[0]["id"], 1);
    assert_eq!(out[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert!(out[0]["result"]["capabilities"]["tools"].is_object());
    assert_eq!(out[0]["result"]["serverInfo"]["name"], "rill-mcp");
    assert_eq!(out[1]["id"], 2);
    assert_eq!(out[1]["result"], json!({}));
}

#[tokio::test]
async fn tools_list_has_every_tool_with_schemas() {
    let h = harness().await;
    let out = roundtrip(
        &h,
        "{\"jsonrpc\":\"2.0\",\"id\":\"x\",\"method\":\"tools/list\"}\n",
    )
    .await;
    let tools = out[0]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "rill_repo_info",
        "rill_list_dir",
        "rill_read_file",
        "rill_log",
        "rill_blame",
        "rill_diff",
        "rill_grep",
        "rill_apply_edits",
        "rill_create_change_from_edits",
        "rill_rebase",
        "rill_resolve",
        "rill_submit",
        "rill_vote",
        "rill_comment",
        "rill_evaluate",
        "rill_request_land",
        "rill_land_status",
        "rill_trunk_status",
        "rill_intent_create",
        "rill_intent_list",
        "rill_intent_get",
        "rill_intent_link",
        "rill_intent_similar",
        "rill_workspace_create",
        "rill_workspace_diff",
        "rill_workspace_handoff",
        "rill_whoami",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    for t in tools {
        let schema = &t["inputSchema"];
        assert_eq!(schema["type"], "object", "{}", t["name"]);
        assert!(schema["properties"].is_object(), "{}", t["name"]);
        assert!(schema["required"].is_array(), "{}", t["name"]);
        for r in schema["required"].as_array().unwrap() {
            assert!(
                schema["properties"].get(r.as_str().unwrap()).is_some(),
                "{} requires undeclared {r}",
                t["name"]
            );
        }
        assert!(!t["description"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn unknown_tool_is_a_jsonrpc_error() {
    let h = harness().await;
    let out = roundtrip(
        &h,
        "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"rill_nope\",\"arguments\":{}}}\n",
    )
    .await;
    assert_eq!(out[0]["id"], 5);
    assert_eq!(out[0]["error"]["code"], INVALID_PARAMS);
    assert!(out[0].get("result").is_none());
}

#[tokio::test]
async fn bad_input_never_stops_the_loop() {
    let h = harness().await;
    let input = concat!(
        "this is not json\n",
        "[1,2]\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"no/such"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rill_read_file","arguments":{"at":"trunk"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rill_read_file","arguments":"nope"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#,
        "\n",
    );
    let out = roundtrip(&h, input).await;
    assert_eq!(out.len(), 6);
    assert_eq!(out[0]["error"]["code"], PARSE_ERROR);
    assert_eq!(out[0]["id"], Value::Null);
    assert!(out[1]["error"].is_object());
    assert_eq!(out[2]["error"]["code"], METHOD_NOT_FOUND);
    // Missing required argument: a tool error result, not a protocol error.
    assert_eq!(out[3]["result"]["isError"], true);
    assert_eq!(
        out[3]["result"]["structuredContent"]["error"]["code"],
        "invalid_arguments"
    );
    assert_eq!(out[4]["result"]["isError"], true);
    assert_eq!(out[5]["result"], json!({}));
}

#[tokio::test]
async fn read_file_returns_content() {
    let h = harness().await;
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rill_read_file","arguments":{"path":"README.md"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rill_read_file","arguments":{"path":"src/big.bin","repo":"other"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rill_read_file","arguments":{"path":"missing.txt"}}}"#,
        "\n",
    );
    let out = roundtrip(&h, input).await;
    let first = &out[0]["result"];
    assert_eq!(first["isError"], false);
    assert_eq!(first["structuredContent"]["encoding"], "utf8");
    assert_eq!(first["structuredContent"]["text"], "# hello\nfn main\n");
    assert_eq!(first["structuredContent"]["path"], "README.md");
    assert_eq!(first["content"][0]["type"], "text");
    let text: Value = serde_json::from_str(first["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["text"], "# hello\nfn main\n");

    // A blob above the inline threshold streams back; binary content is hex.
    let second = &out[1]["result"]["structuredContent"];
    assert_eq!(second["encoding"], "hex");
    assert_eq!(second["size"], 300);

    let third = &out[2]["result"];
    assert_eq!(third["isError"], true);
    assert_eq!(third["structuredContent"]["error"]["code"], "not_found");
}

#[tokio::test]
async fn list_log_and_grep_tools() {
    let h = harness().await;
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rill_list_dir","arguments":{}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rill_log","arguments":{"limit":5}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rill_grep","arguments":{"pattern":"fn main","ignore_case":true}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rill_repo_info","arguments":{}}}"#,
        "\n",
    );
    let out = roundtrip(&h, input).await;
    let entries = out[0]["result"]["structuredContent"]["entries"]
        .as_array()
        .unwrap();
    assert_eq!(entries[0]["name"], "README.md");
    assert_eq!(entries[0]["type"], "file");
    assert_eq!(entries[1]["name"], "src");
    assert_eq!(entries[1]["type"], "dir");

    let log = &out[1]["result"]["structuredContent"]["entries"];
    assert_eq!(log[0]["summary"], "change a");
    assert_eq!(log[0]["trunk_seq"], 3);
    assert_eq!(log[0]["author"]["name"], "alice");

    let grep = &out[2]["result"]["structuredContent"];
    let matches = grep["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0]["path"], "README.md");
    assert_eq!(matches[0]["line_no"], 2);
    assert_eq!(matches[1]["path"], "src/lib.rs");
    assert_eq!(grep["truncated"], false);

    assert_eq!(out[3]["result"]["structuredContent"]["name"], "main");
    assert_eq!(out[3]["result"]["structuredContent"]["trunk_head_seq"], 3);
}
