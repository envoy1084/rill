//! The Connect protocol: plain HTTP/1.1 with JSON, as a browser or `curl`
//! calls the API, from the same handlers as gRPC.

use crate::common::http_post;
use crate::*;

fn api_addr(server: &TestServer) -> std::net::SocketAddr {
    server.grpc.trim_start_matches("http://").parse().unwrap()
}

fn json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(body)))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_json_call_over_http1_reaches_the_api_and_errors_carry_their_code() {
    let server = TestServer::start(dev_config()).await;
    let addr = api_addr(&server);
    let call = |method: &'static str, body: &'static str, auth: Option<&'static str>| {
        let mut headers = vec![("Content-Type", "application/json")];
        if let Some(auth) = auth {
            headers.push(("Authorization", auth));
        }
        async move {
            http_post(
                addr,
                &format!("/rill.v1.RepoService/{method}"),
                &headers,
                body.as_bytes(),
            )
            .await
        }
    };

    let (status, _, body) = call("CreateRepo", r#"{"repo":"web"}"#, None).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json(&body)["repo"]["name"], "web");

    let (status, _, body) = call("GetRepo", r#"{"repo":"web"}"#, None).await;
    assert_eq!(status, 200);
    assert_eq!(json(&body)["repo"]["name"], "web");

    let (status, _, body) = call("GetRepo", r#"{"repo":"missing"}"#, None).await;
    assert_eq!(status, 404);
    let error = json(&body);
    assert_eq!(error["code"], "not_found");
    let info = error["details"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["type"] == "google.rpc.ErrorInfo")
        .expect("the reason travels as an ErrorInfo detail");
    assert_eq!(info["debug"]["reason"], "NOT_FOUND", "{error}");

    let (status, _, body) = call("GetRepo", r#"{"repo":"web"}"#, Some("Bearer nonsense")).await;
    assert_eq!(status, 401, "a bad token is refused before any handler");
    assert_eq!(json(&body)["code"], "unauthenticated");
    server.stop().await;
}
