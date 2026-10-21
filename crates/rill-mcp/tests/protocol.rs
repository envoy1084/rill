//! The MCP server end to end: a client initializes, lists and calls tools
//! over a pipe, against a mock repository. The protocol itself is rmcp's.

mod common;

use common::{sample_info, Harness, MockRepo};
use serde_json::{json, Value};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};

/// A minimal MCP client speaking newline-delimited JSON-RPC.
struct Client {
    write: WriteHalf<DuplexStream>,
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
}

impl Client {
    async fn notify(&mut self, message: Value) {
        let line = format!("{message}\n");
        self.write.write_all(line.as_bytes()).await.unwrap();
    }

    /// Send a request and return its `result`.
    async fn ask(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.notify(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let line = self
                .lines
                .next_line()
                .await
                .unwrap()
                .expect("server closed");
            let answer: Value = serde_json::from_str(&line).unwrap();
            if answer["id"] == id {
                return answer["result"].clone();
            }
        }
    }
}

#[tokio::test]
async fn a_client_initializes_lists_and_calls_tools() {
    let repo = MockRepo::default();
    repo.set_tree("trunk", vec![("README.md", b"# hello\n")]);
    repo.log.lock().unwrap().push(sample_info("a", Some(3)));
    let h = Harness::start(repo).await;
    let (client, server_side) = tokio::io::duplex(1 << 20);
    let server = h.server.clone();
    tokio::spawn(async move { rill_mcp::serve(server, tokio::io::split(server_side)).await });
    let (read, write) = tokio::io::split(client);
    let mut c = Client {
        write,
        lines: BufReader::new(read).lines(),
    };

    let init = c
        .ask(
            1,
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }),
        )
        .await;
    assert_eq!(init["serverInfo"]["name"], "rill-mcp", "{init}");
    assert!(init["capabilities"]["tools"].is_object());
    c.notify(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await;

    let tools = c.ask(2, "tools/list", json!({})).await;
    let tools = tools["tools"].as_array().unwrap();
    assert_eq!(tools.len(), rill_mcp::tools::definitions().len());
    assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));

    let read = c
        .ask(
            3,
            "tools/call",
            json!({ "name": "rill_read_file", "arguments": { "repo": "main", "path": "README.md" } }),
        )
        .await;
    assert_eq!(read["isError"], false, "{read}");
    assert!(
        read["structuredContent"].to_string().contains("# hello"),
        "{read}"
    );

    let missing = c
        .ask(
            4,
            "tools/call",
            json!({ "name": "rill_read_file", "arguments": { "repo": "main" } }),
        )
        .await;
    assert_eq!(
        missing["isError"], true,
        "a bad call is a tool error: {missing}"
    );
}
