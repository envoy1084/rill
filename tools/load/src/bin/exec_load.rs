//! `exec_load`: verification-shaped execution load through REAPI.
//!
//! Steps re-run on trees whose inputs did not change most of the time
//! (cache hits) and on changed inputs the rest (executions). The tool
//! seeds a pool of executed actions, then submits at a fixed rate for a
//! while: `--hit-share` of requests repeat a pooled action, the rest are
//! new actions whose input file is new. It reports the achieved rate, the
//! share served from the cache and latency by kind.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use clap::Parser;
use futures::StreamExt;
use hdrhistogram::Histogram;
use parking_lot::Mutex;
use prost::Message;
use rand::RngExt;
use rill_reapi_proto::reapi::content_addressable_storage_client::ContentAddressableStorageClient;
use rill_reapi_proto::reapi::execution_client::ExecutionClient;
use rill_reapi_proto::reapi::{
    batch_update_blobs_request, command, digest_function, platform, Action,
    BatchUpdateBlobsRequest, Command, Digest, Directory, ExecuteRequest, ExecuteResponse, FileNode,
    Platform,
};
use rill_sdk::testing::DevServer;
use tokio::sync::Semaphore;
use tonic::transport::Channel;

#[derive(Parser)]
#[command(about = "Verification-shaped load on remote execution")]
struct Args {
    /// A server's gRPC endpoint; without one an in-process dev server
    /// with `--local-workers` runs.
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long, env = "RILL_TOKEN")]
    token: Option<String>,
    /// Workers of the in-process server.
    #[arg(long, default_value_t = 16)]
    local_workers: usize,
    /// Platform properties, `name=value`, repeatable.
    #[arg(long = "platform")]
    platform: Vec<String>,
    /// Requests per second.
    #[arg(long, default_value_t = 175.0)]
    rate: f64,
    /// The share of requests that repeat an action already run.
    #[arg(long, default_value_t = 0.85)]
    hit_share: f64,
    /// Actions run before measuring, for requests to repeat.
    #[arg(long, default_value_t = 200)]
    seed: usize,
    #[arg(long, default_value_t = 90)]
    seconds: u64,
    /// Requests in flight at most.
    #[arg(long, default_value_t = 1024)]
    concurrency: usize,
}

fn digest(bytes: &[u8]) -> Digest {
    Digest {
        hash: blake3::hash(bytes).to_hex().to_string(),
        size_bytes: bytes.len() as i64,
    }
}

#[derive(Clone)]
struct Client {
    cas: ContentAddressableStorageClient<
        tonic::service::interceptor::InterceptedService<Channel, Bearer>,
    >,
    execution: ExecutionClient<tonic::service::interceptor::InterceptedService<Channel, Bearer>>,
    platform: Platform,
}

#[derive(Clone)]
struct Bearer(Option<String>);

impl tonic::service::Interceptor for Bearer {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(token) = &self.0 {
            let value = format!("Bearer {token}")
                .parse()
                .map_err(|_| tonic::Status::internal("token"))?;
            request.metadata_mut().insert("authorization", value);
        }
        Ok(request)
    }
}

impl Client {
    /// Upload a step reading one input file of `content` and return its
    /// action digest.
    async fn upload(&mut self, content: &[u8]) -> anyhow::Result<Digest> {
        let input = Bytes::copy_from_slice(content);
        let root = Directory {
            files: vec![FileNode {
                name: "input.txt".into(),
                digest: Some(digest(&input)),
                is_executable: false,
                node_properties: None,
            }],
            ..Default::default()
        }
        .encode_to_vec();
        let command = Command {
            arguments: vec!["sh".into(), "-c".into(), "wc -c < input.txt > count".into()],
            // Workers give a command only its own environment.
            environment_variables: vec![command::EnvironmentVariable {
                name: "PATH".into(),
                value: "/usr/local/bin:/usr/bin:/bin".into(),
            }],
            output_paths: vec!["count".into()],
            ..Default::default()
        }
        .encode_to_vec();
        let action = Action {
            command_digest: Some(digest(&command)),
            input_root_digest: Some(digest(&root)),
            timeout: Some(prost_types::Duration {
                seconds: 60,
                nanos: 0,
            }),
            platform: Some(self.platform.clone()),
            ..Default::default()
        }
        .encode_to_vec();
        let action_digest = digest(&action);
        let requests = [input.to_vec(), root, command, action]
            .into_iter()
            .map(|data| batch_update_blobs_request::Request {
                digest: Some(digest(&data)),
                data: Bytes::from(data),
                compressor: 0,
            })
            .collect();
        self.cas
            .batch_update_blobs(BatchUpdateBlobsRequest {
                instance_name: String::new(),
                requests,
                digest_function: digest_function::Value::Blake3 as i32,
            })
            .await?;
        Ok(action_digest)
    }

    /// Execute `action` and wait for its response.
    async fn execute(&mut self, action: &Digest) -> anyhow::Result<ExecuteResponse> {
        let mut stream = self
            .execution
            .execute(ExecuteRequest {
                action_digest: Some(action.clone()),
                digest_function: digest_function::Value::Blake3 as i32,
                ..Default::default()
            })
            .await?
            .into_inner();
        while let Some(op) = stream.next().await {
            let op = op?;
            if !op.done {
                continue;
            }
            let Some(rill_reapi_proto::longrunning::operation::Result::Response(any)) = op.result
            else {
                anyhow::bail!("operation {} ended without a response", op.name);
            };
            let response = ExecuteResponse::decode(any.value.as_slice())?;
            if let Some(status) = response.status.as_ref().filter(|s| s.code != 0) {
                anyhow::bail!("action failed: {}", status.message);
            }
            return Ok(response);
        }
        anyhow::bail!("the stream ended before the operation")
    }
}

#[derive(Default)]
struct Tally {
    hits: u64,
    executed: u64,
    errors: u64,
    errors_seen: HashMap<String, u64>,
}

async fn embedded(workers: usize) -> anyhow::Result<DevServer> {
    let mut config = DevServer::config();
    config.exec.local.workers = workers;
    Ok(DevServer::start_with(config).await?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let (endpoint, _server) = match &args.endpoint {
        Some(e) => (e.clone(), None),
        None => {
            let server = embedded(args.local_workers).await?;
            (server.endpoint().to_string(), Some(server))
        }
    };
    let channel = loop {
        match Channel::from_shared(endpoint.clone())?.connect().await {
            Ok(c) => break c,
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    };
    let bearer = Bearer(args.token.clone());
    let wrap =
        || tonic::service::interceptor::InterceptedService::new(channel.clone(), bearer.clone());
    let platform = Platform {
        properties: args
            .platform
            .iter()
            .filter_map(|p| p.split_once('='))
            .map(|(n, v)| platform::Property {
                name: n.into(),
                value: v.into(),
            })
            .collect(),
    };
    let client = Client {
        cas: ContentAddressableStorageClient::new(wrap()),
        execution: ExecutionClient::new(wrap()),
        platform,
    };
    let run_id: u64 = rand::rng().random();

    // Seed: actions whose results later requests repeat.
    let started = Instant::now();
    let pool = Arc::new(Mutex::new(Vec::with_capacity(args.seed)));
    let seeding = Arc::new(Semaphore::new(64));
    let mut tasks = Vec::new();
    for i in 0..args.seed {
        let (mut client, pool, permit) = (
            client.clone(),
            pool.clone(),
            seeding.clone().acquire_owned().await?,
        );
        tasks.push(tokio::spawn(async move {
            let action = client
                .upload(format!("seed {run_id} {i}\n").as_bytes())
                .await?;
            client.execute(&action).await?;
            pool.lock().push(action);
            drop(permit);
            anyhow::Ok(())
        }));
    }
    for task in tasks {
        task.await??;
    }
    println!(
        "seeded {} actions in {:.1}s",
        args.seed,
        started.elapsed().as_secs_f64()
    );

    let tally = Arc::new(Mutex::new(Tally::default()));
    let hit_latency = Arc::new(Mutex::new(Histogram::<u64>::new(3)?));
    let miss_latency = Arc::new(Mutex::new(Histogram::<u64>::new(3)?));
    let limit = Arc::new(Semaphore::new(args.concurrency));
    let interval = Duration::from_secs_f64(1.0 / args.rate);
    let deadline = Instant::now() + Duration::from_secs(args.seconds);
    let measuring = Instant::now();
    let mut next = Instant::now();
    let mut sent = 0u64;
    let mut handles = Vec::new();
    while Instant::now() < deadline {
        tokio::time::sleep_until(next.into()).await;
        next += interval;
        sent += 1;
        let repeat = rand::rng().random::<f64>() < args.hit_share;
        let Ok(permit) = limit.clone().try_acquire_owned() else {
            tally.lock().errors += 1;
            continue;
        };
        let (mut client, pool, tally) = (client.clone(), pool.clone(), tally.clone());
        let (hits, misses) = (hit_latency.clone(), miss_latency.clone());
        handles.push(tokio::spawn(async move {
            let began = Instant::now();
            let outcome = async {
                let action = if repeat {
                    let pool = pool.lock();
                    pool[rand::rng().random_range(0..pool.len())].clone()
                } else {
                    client
                        .upload(format!("change {run_id} {sent}\n").as_bytes())
                        .await?
                };
                let response = client.execute(&action).await?;
                if !repeat {
                    pool.lock().push(action);
                }
                anyhow::Ok(response.cached_result)
            }
            .await;
            let micros = began.elapsed().as_micros() as u64;
            let mut t = tally.lock();
            match outcome {
                Ok(true) => {
                    t.hits += 1;
                    hits.lock().record(micros).ok();
                }
                Ok(false) => {
                    t.executed += 1;
                    misses.lock().record(micros).ok();
                }
                Err(e) => {
                    t.errors += 1;
                    *t.errors_seen.entry(e.to_string()).or_default() += 1;
                }
            }
            drop(permit);
        }));
    }
    for handle in handles {
        handle.await?;
    }
    let elapsed = measuring.elapsed().as_secs_f64();
    let t = tally.lock();
    let done = t.hits + t.executed;
    let (h, m) = (hit_latency.lock(), miss_latency.lock());
    println!(
        "requests {sent} in {elapsed:.1}s ({:.1}/s), completed {done}",
        done as f64 / elapsed
    );
    println!(
        "cache hits {} ({:.1}%), executed {} ({:.1}/s), errors {}",
        t.hits,
        100.0 * t.hits as f64 / done.max(1) as f64,
        t.executed,
        t.executed as f64 / elapsed,
        t.errors
    );
    println!(
        "hit latency p50 {:.1} ms p99 {:.1} ms; executed p50 {:.1} ms p99 {:.1} ms",
        h.value_at_quantile(0.5) as f64 / 1000.0,
        h.value_at_quantile(0.99) as f64 / 1000.0,
        m.value_at_quantile(0.5) as f64 / 1000.0,
        m.value_at_quantile(0.99) as f64 / 1000.0
    );
    for (error, count) in &t.errors_seen {
        println!("  {count} × {error}");
    }
    Ok(())
}
