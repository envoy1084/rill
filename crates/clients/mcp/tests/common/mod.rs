//! A dev server with a repository, and a minimal MCP client speaking
//! newline-delimited JSON-RPC to a server over an in-memory pipe.

#![allow(dead_code)]

use rill_mcp::{McpServer, ToolContext};
use rill_sdk::testing::DevServer;
use rill_sdk::Client as SdkClient;
use serde_json::{json, Value};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};

/// The repository every test works in.
pub const REPO: &str = "main";

/// A dev server holding [`REPO`], and an SDK client for it.
pub struct Fixture {
    _server: DevServer,
    pub sdk: SdkClient,
}

impl Fixture {
    pub async fn start() -> Self {
        let server = DevServer::start().await.unwrap();
        let sdk = server.client().await.unwrap();
        sdk.create_repo(REPO, None).await.unwrap();
        Fixture {
            _server: server,
            sdk,
        }
    }

    /// An initialized MCP session against the server.
    pub async fn session(&self) -> Session {
        let ctx = ToolContext::new(self.sdk.clone(), Some(REPO.into()));
        Session::start(McpServer::new(ctx)).await
    }
}

/// One MCP session.
pub struct Session {
    write: WriteHalf<DuplexStream>,
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    next_id: i64,
    /// Notifications received while waiting for answers.
    pub notifications: Vec<Value>,
}

impl Session {
    async fn start(server: McpServer) -> Self {
        let (client, server_side) = tokio::io::duplex(1 << 22);
        tokio::spawn(async move { rill_mcp::serve(server, tokio::io::split(server_side)).await });
        let (read, write) = tokio::io::split(client);
        let mut s = Session {
            write,
            lines: BufReader::new(read).lines(),
            next_id: 0,
            notifications: Vec::new(),
        };
        let init = s
            .ask(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" },
                }),
            )
            .await;
        assert_eq!(init["result"]["serverInfo"]["name"], "rill-mcp", "{init}");
        s.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        s
    }

    async fn send(&mut self, message: Value) {
        let line = format!("{message}\n");
        self.write.write_all(line.as_bytes()).await.unwrap();
    }

    /// Send a request and return the whole response.
    pub async fn ask(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let line = self
                .lines
                .next_line()
                .await
                .unwrap()
                .expect("server closed");
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
            self.notifications.push(message);
        }
    }

    /// Call a tool; the structured result and whether it is an error.
    pub async fn call_raw(&mut self, tool: &str, args: Value) -> (Value, bool) {
        let answer = self
            .ask("tools/call", json!({ "name": tool, "arguments": args }))
            .await;
        let result = &answer["result"];
        assert!(result.is_object(), "{tool}: {answer}");
        (
            result["structuredContent"].clone(),
            result["isError"].as_bool().unwrap_or(false),
        )
    }

    /// Wait up to five seconds for a notification with `method`.
    pub async fn notification(&mut self, method: &str) -> Value {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(i) = self
                .notifications
                .iter()
                .position(|n| n["method"] == method)
            {
                return self.notifications.remove(i);
            }
            let line = tokio::time::timeout_at(deadline, self.lines.next_line())
                .await
                .unwrap_or_else(|_| panic!("no {method} notification"))
                .unwrap()
                .expect("server closed");
            self.notifications
                .push(serde_json::from_str(&line).unwrap());
        }
    }

    /// Submit, approve and land the session's change.
    pub async fn land(&mut self, message: &str) -> Value {
        self.call("rill_commit", json!({ "message": message }))
            .await;
        self.call(
            "rill_respond",
            json!({ "actions": [
                { "action": "vote", "label": "Review", "value": 2 },
                { "action": "vote", "label": "Verified", "value": 1 },
            ] }),
        )
        .await;
        let landed = self.call("rill_land", json!({ "wait_seconds": 30 })).await;
        assert_eq!(landed["outcome"], "landed", "{landed}");
        landed
    }

    /// Call a tool that must succeed.
    pub async fn call(&mut self, tool: &str, args: Value) -> Value {
        let (value, failed) = self.call_raw(tool, args).await;
        assert!(!failed, "{tool} failed: {value}");
        value
    }
}
