//! A round-robin TCP load balancer for tests. Each client connection goes
//! to the next live backend. It can also deliver one request and drop its
//! reply, as when a server dies after doing the work and before answering.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Default)]
struct State {
    backends: Vec<SocketAddr>,
    alive: Vec<bool>,
    next: usize,
    cut_next: bool,
    cut: Option<usize>,
}

pub struct Balancer {
    pub addr: SocketAddr,
    state: Arc<Mutex<State>>,
}

impl Balancer {
    pub async fn start(backends: Vec<SocketAddr>) -> Balancer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State {
            alive: vec![true; backends.len()],
            backends,
            ..State::default()
        }));
        let shared = state.clone();
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let state = shared.clone();
                tokio::spawn(async move {
                    if let Some((index, backend)) = pick(&state).await {
                        proxy(client, backend, index, state).await;
                    }
                });
            }
        });
        Balancer { addr, state }
    }

    /// Deliver the next request any connection carries, never return its
    /// reply, and close that connection.
    pub fn cut_next_request(&self) {
        self.state.lock().cut_next = true;
    }

    /// Which backend got the request whose reply was dropped.
    pub fn take_cut(&self) -> Option<usize> {
        self.state.lock().cut.take()
    }

    pub fn mark_dead(&self, index: usize) {
        self.state.lock().alive[index] = false;
    }
}

async fn pick(state: &Mutex<State>) -> Option<(usize, TcpStream)> {
    let count = state.lock().backends.len();
    for _ in 0..count {
        let (index, addr) = {
            let mut s = state.lock();
            let index = s.next % count;
            s.next += 1;
            if !s.alive[index] {
                continue;
            }
            (index, s.backends[index])
        };
        if let Ok(stream) = TcpStream::connect(addr).await {
            return Some((index, stream));
        }
    }
    None
}

async fn proxy(client: TcpStream, backend: TcpStream, index: usize, state: Arc<Mutex<State>>) {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut backend_read, mut backend_write) = backend.into_split();
    let dropping = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let silent = dropping.clone();
    let replies = async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = match backend_read.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            if silent.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            if client_write.write_all(&buf[..n]).await.is_err() {
                return;
            }
        }
    };
    let requests = async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = match client_read.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let cut = {
                let mut s = state.lock();
                let cut = std::mem::take(&mut s.cut_next);
                if cut {
                    s.cut = Some(index);
                }
                cut
            };
            if cut {
                dropping.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            if backend_write.write_all(&buf[..n]).await.is_err() {
                return;
            }
            if cut {
                // Let the backend do the work, then hang up on the client.
                tokio::time::sleep(Duration::from_millis(300)).await;
                return;
            }
        }
    };
    tokio::select! {
        _ = replies => {}
        _ = requests => {}
    }
}
