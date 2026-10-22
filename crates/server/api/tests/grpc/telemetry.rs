//! What a server reports: request ids, RPC and store metrics.

use super::*;
use connectrpc::client::CallOptions;

#[tokio::test]
async fn requests_carry_ids_and_show_up_in_metrics() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    let request = pb::CreateRepoRequest {
        operation_id: Bytes::new(),
        repo: "observed".into(),
        description: None,
        windows_safe: false,
    };
    let options = CallOptions::default().with_header("x-request-id", "req-42");
    let response = c
        .repo
        .create_repo_with_options(request, options)
        .await
        .unwrap();
    assert_eq!(response.headers().get("x-request-id").unwrap(), "req-42");
    let missing = c
        .repo
        .get_repo(pb::GetRepoRequest {
            repo: "missing".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(missing.code, Code::NotFound);
    assert!(
        missing.response_headers().get("x-request-id").is_some(),
        "a request without an id gets one"
    );

    let (status, _, body) = http_get(server.http, "/metrics").await;
    assert_eq!(status, 200);
    let text = String::from_utf8(body).unwrap();
    for line in [
        r#"rill_rpc_requests_total{service="rill.v1.RepoService",method="CreateRepo",code="Ok"} 1"#,
        r#"rill_rpc_requests_total{service="rill.v1.RepoService",method="GetRepo",code="NotFound"} 1"#,
    ] {
        assert!(text.contains(line), "missing {line} in\n{text}");
    }
    assert!(text.contains(r#"rill_meta_transactions_total{outcome="committed"}"#));
    server.stop().await;
}

#[test]
fn dashboards_query_only_metrics_the_server_exports() {
    let exported: std::collections::BTreeSet<String> = rill_runtime::telemetry::Metrics::new()
        .names()
        .iter()
        .cloned()
        .collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../deploy/grafana");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let dashboard: serde_json::Value = serde_json::from_str(&text).unwrap();
        for panel in dashboard["panels"].as_array().unwrap() {
            for target in panel["targets"].as_array().unwrap() {
                let expr = target["expr"].as_str().unwrap();
                for name in metric_names(expr) {
                    let family = ["_bucket", "_sum", "_count", "_total"]
                        .iter()
                        .find_map(|s| name.strip_suffix(s))
                        .unwrap_or(&name);
                    assert!(
                        exported.contains(family),
                        "{} queries {name}, which no server exports",
                        path.display()
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(
        checked > 10,
        "no dashboard queries found in {}",
        dir.display()
    );
}

fn metric_names(expr: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = expr;
    while let Some(start) = rest.find("rill_") {
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        names.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    names
}

#[test]
fn shipped_configurations_parse() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let config = ServerConfig::load(Some(&root.join("deploy/compose/rill.toml"))).unwrap();
    assert!(!config.dev_mode);
    assert!(matches!(
        config.metadata,
        rill_runtime::config::MetadataConfig::Fdb { .. }
    ));
    assert!(matches!(config.storage, rill_api::StorageConfig::S3 { .. }));
}
