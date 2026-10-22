//! `access_probe`: how soon a relation change made on one server is
//! honoured by the others. Grants and revokes a reader on the first
//! endpoint, then polls the others as that reader: once with a fresh
//! client (the servers learn of it from the event log), once with a client
//! that sends the relation version the revoke returned (the next request
//! must already be refused).

use std::time::{Duration, Instant};

use clap::Parser;
use rill_proto::rill::v1 as pb;
use rill_sdk::{Client, ClientConfig, Code, Root};
use rill_types::{Principal, RepoPathBuf};

#[derive(Parser)]
#[command(about = "Measure how fast relation changes reach every server")]
struct Args {
    /// The server relations are changed on.
    #[arg(long)]
    write: String,
    /// Servers polled as the reader (comma separated).
    #[arg(long, value_delimiter = ',')]
    check: Vec<String>,
    /// A token for an administrator of the repository's org.
    #[arg(long, env = "RILL_ADMIN_TOKEN")]
    admin_token: String,
    /// A token for the reader.
    #[arg(long, env = "RILL_READER_TOKEN")]
    reader_token: String,
    /// The reader's principal name (a human).
    #[arg(long)]
    reader: String,
    #[arg(long, default_value = "access-probe")]
    repo: String,
    #[arg(long, default_value_t = 20)]
    rounds: usize,
}

async fn client(endpoint: &str, token: &str) -> Client {
    let mut config = ClientConfig::new(endpoint.to_string());
    config.token = Some(token.to_string());
    Client::connect(config).await.expect("connect")
}

/// Whether the reader can list the repository root.
async fn can_read(reader: &Client, repo: &str) -> bool {
    match reader
        .repo(repo)
        .list_dir(&Root::trunk(), &RepoPathBuf::root())
        .await
    {
        Ok(_) => true,
        Err(rill_sdk::Error::Status {
            code: Code::PermissionDenied,
            ..
        }) => false,
        // An empty trunk lists as not found once access is allowed.
        Err(rill_sdk::Error::Status {
            code: Code::NotFound,
            ..
        }) => true,
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// Poll until the reader's access on `endpoint` is `want`; how long it took.
async fn until(endpoint: &str, token: &str, repo: &str, want: bool, since: Instant) -> Duration {
    let reader = client(endpoint, token).await;
    loop {
        if can_read(&reader, repo).await == want {
            return since.elapsed();
        }
        if since.elapsed() > Duration::from_secs(30) {
            panic!("{endpoint} did not see the change within 30 s");
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let admin = client(&args.write, &args.admin_token).await;
    let _ = admin.create_repo(&args.repo, None).await;
    let tuple = pb::Relation {
        repo: args.repo.clone(),
        path: String::new(),
        relation: "reader".into(),
        subject: format!("principal:{}", Principal::human(&args.reader).id),
    };
    let mut fresh = Vec::new();
    let mut first_refused = 0;
    for round in 0..args.rounds {
        admin
            .access_svc()
            .add_relation(pb::AddRelationRequest {
                operation_id: Default::default(),
                relation: Some(tuple.clone()),
            })
            .await
            .expect("grant");
        for endpoint in &args.check {
            until(
                endpoint,
                &args.reader_token,
                &args.repo,
                true,
                Instant::now(),
            )
            .await;
        }
        let revoked = admin
            .access_svc()
            .remove_relation(pb::RemoveRelationRequest {
                operation_id: Default::default(),
                relation: Some(tuple.clone()),
            })
            .await
            .expect("revoke")
            .into_inner();
        let t0 = Instant::now();
        // A client that knows the version: refused on its first request.
        for endpoint in &args.check {
            let reader = client(endpoint, &args.reader_token).await;
            reader
                .relations_seen()
                .observe(&revoked.repo_id, revoked.relation_version);
            if !can_read(&reader, &args.repo).await {
                first_refused += 1;
            }
        }
        // A fresh client: refused once its server has followed the log.
        let polls = args
            .check
            .iter()
            .map(|e| until(e, &args.reader_token, &args.repo, false, t0));
        fresh.extend(futures::future::join_all(polls).await);
        eprintln!("round {round}: done");
    }
    fresh.sort();
    let checks = args.rounds * args.check.len();
    println!(
        "revoke refused on the first request with the version header: {first_refused}/{checks}"
    );
    println!(
        "revoke refused without it after: p50 {:?}, p99 {:?}, max {:?} ({} samples; includes the version-header probes)",
        percentile(&fresh, 0.5),
        percentile(&fresh, 0.99),
        fresh.last().unwrap(),
        fresh.len()
    );
}
