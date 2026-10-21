//! A server on its own thread and runtime, so killing it drops every task
//! it runs at once, without releasing leases or claims: a crash.

use std::net::SocketAddr;
use std::sync::mpsc;
use std::thread::JoinHandle;

use rill_api::{AppState, Listeners, ServerConfig};
use tokio_util::sync::CancellationToken;

pub struct Replica {
    pub grpc: SocketAddr,
    pub http: SocketAddr,
    kill: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Replica {
    pub fn start(config: ServerConfig, stores: rill_store::Stores) -> Replica {
        let (ready, addrs) = mpsc::channel();
        let (kill, killed) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            let bound = runtime.block_on(async move {
                let listeners = Listeners::bind(
                    "127.0.0.1:0".parse().unwrap(),
                    "127.0.0.1:0".parse().unwrap(),
                )
                .await
                .unwrap();
                let bound = listeners.addrs().unwrap();
                let state =
                    AppState::with_stores(config, stores, CancellationToken::new()).unwrap();
                state.repos.warm().await.unwrap();
                tokio::spawn(rill_api::serve_until(
                    state,
                    listeners,
                    CancellationToken::new(),
                ));
                bound
            });
            ready.send(bound).unwrap();
            // Until killed, or until the test drops this replica.
            let _ = killed.recv();
            runtime.shutdown_background();
        });
        let (grpc, http) = addrs.recv().unwrap();
        Replica {
            grpc,
            http,
            kill: Some(kill),
            thread: Some(thread),
        }
    }

    /// Crash: every task of this server stops where it is.
    pub fn kill(&mut self) {
        drop(self.kill.take());
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

impl Drop for Replica {
    fn drop(&mut self) {
        self.kill();
    }
}
