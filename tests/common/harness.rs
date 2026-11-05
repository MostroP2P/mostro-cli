//! Process harness shared by the order-list and order-info process tests.
//!
//! Every child runs the real `mostro-cli` binary with an allow-listed
//! environment, sandboxed HOME/USERPROFILE/MCLI_DIR and working directory,
//! closed stdin, captured streams and a parent watchdog. Relays are in-process
//! nostr-sdk `LocalRelay`s behind a loopback listener on an OS-assigned port.
//! Included with `#[path]`; each test crate uses a different subset.
#![allow(dead_code)]

use std::convert::Infallible;
use std::ffi::OsStr;
use std::future::pending;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs, io, process};

use hyper::body::Incoming;
use hyper::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::{self, OnUpgrade};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use nostr_sdk::prelude::{Keys, LocalRelay};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Notify;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;
use tungstenite::handshake::derive_accept_key;

/// Parent-side limit for one child run.
pub const WATCHDOG: Duration = Duration::from_secs(40);
/// The only parent variables a child inherits; Windows needs SYSTEMROOT to
/// initialize sockets.
pub const INHERITED_ENV: &[&str] = &["SYSTEMROOT"];
/// Variables pointing every identity location into the sandbox.
pub const SANDBOX_ENV: &[&str] = &["HOME", "USERPROFILE", "MCLI_DIR"];

pub static SANDBOX_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Per-test directories, removed on drop, including when an assertion panics.
pub struct Sandbox {
    pub root: PathBuf,
}

impl Sandbox {
    pub fn new() -> io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let seq = SANDBOX_SEQ.fetch_add(1, Ordering::SeqCst);
        let root = env::temp_dir().join(format!("mcli-process-{}-{seq}-{nanos}", process::id()));
        // `create_dir` fails instead of reusing a directory that already exists.
        fs::create_dir(&root)?;
        let sandbox = Self { root };
        for dir in [sandbox.home(), sandbox.mcli_dir(), sandbox.cwd()] {
            fs::create_dir(dir)?;
        }
        Ok(sandbox)
    }

    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    pub fn mcli_dir(&self) -> PathBuf {
        self.root.join("mcli")
    }

    pub fn cwd(&self) -> PathBuf {
        self.root.join("cwd")
    }

    /// A `mostro-cli` command that sees only the allow-listed and sandbox
    /// environment. Node configuration is added separately by the test.
    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mostro-cli"));
        command.args(args).env_clear();
        for key in INHERITED_ENV {
            if let Some(value) = env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("MCLI_DIR", self.mcli_dir())
            .current_dir(self.cwd())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// What the loopback listener does with each upgraded WebSocket.
#[derive(Clone)]
pub enum Backend {
    /// Serve the Nostr protocol from this relay.
    Relay(Box<LocalRelay>),
    /// Complete the upgrade, then never answer.
    Silent,
}

/// Loopback WebSocket endpoint on an OS-assigned port. The socket is bound and
/// listening when `start` returns, so a child can connect without waiting.
pub struct LoopbackRelay {
    pub url: String,
    pub connections: Arc<AtomicUsize>,
    pub open: Arc<AtomicUsize>,
    pub ended: Arc<Notify>,
    pub backend: Backend,
    pub accept: JoinHandle<()>,
}

impl LoopbackRelay {
    pub async fn start(backend: Backend) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let url = format!("ws://{}", listener.local_addr()?);
        let connections = Arc::new(AtomicUsize::new(0));
        let open = Arc::new(AtomicUsize::new(0));
        let ended = Arc::new(Notify::new());
        let accept = tokio::spawn(accept_loop(
            listener,
            backend.clone(),
            Sessions {
                accepted: Arc::clone(&connections),
                open: Arc::clone(&open),
                ended: Arc::clone(&ended),
            },
        ));
        Ok(Self {
            url,
            connections,
            open,
            ended,
            backend,
            accept,
        })
    }

    /// TCP connections accepted so far.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Whether every accepted connection ended within `limit`. A `Silent`
    /// session never notices its peer closing, so use this on relays only.
    pub async fn wait_idle(&self, limit: Duration) -> bool {
        let idle = async {
            loop {
                let ended = self.ended.notified();
                if self.open.load(Ordering::SeqCst) == 0 {
                    return;
                }
                ended.await;
            }
        };
        timeout(limit, idle).await.is_ok()
    }
}

impl Drop for LoopbackRelay {
    fn drop(&mut self) {
        // Aborting the accept task drops its JoinSet, which aborts every session.
        self.accept.abort();
        if let Backend::Relay(relay) = &self.backend {
            relay.shutdown();
        }
    }
}

/// Connection counters shared by a listener and its sessions.
pub struct Sessions {
    pub accepted: Arc<AtomicUsize>,
    pub open: Arc<AtomicUsize>,
    pub ended: Arc<Notify>,
}

pub async fn accept_loop(listener: TcpListener, backend: Backend, counters: Sessions) {
    let mut sessions = JoinSet::new();
    while let Ok((stream, remote)) = listener.accept().await {
        counters.accepted.fetch_add(1, Ordering::SeqCst);
        counters.open.fetch_add(1, Ordering::SeqCst);
        let open = Arc::clone(&counters.open);
        let ended = Arc::clone(&counters.ended);
        let backend = backend.clone();
        sessions.spawn(async move {
            session(stream, remote, backend).await;
            open.fetch_sub(1, Ordering::SeqCst);
            ended.notify_waiters();
        });
    }
}

pub async fn session(stream: TcpStream, remote: SocketAddr, backend: Backend) {
    let (upgrades, mut queued) = unbounded_channel();
    let service = service_fn(move |request| handshake(request, upgrades.clone()));
    let served = http1::Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades()
        .await;
    if served.is_err() {
        return;
    }
    let Some(on_upgrade) = queued.recv().await else {
        return;
    };
    let Ok(upgraded) = on_upgrade.await else {
        return;
    };
    match backend {
        Backend::Relay(relay) => {
            let _ = relay.take_connection(TokioIo::new(upgraded), remote).await;
        }
        Backend::Silent => {
            let _held_open = upgraded;
            pending::<()>().await;
        }
    }
}

pub async fn handshake(
    mut request: Request<Incoming>,
    upgrades: UnboundedSender<OnUpgrade>,
) -> Result<Response<String>, Infallible> {
    let Some(accept) = request
        .headers()
        .get(SEC_WEBSOCKET_KEY)
        .map(|key| derive_accept_key(key.as_bytes()))
    else {
        return Ok(empty_response(StatusCode::BAD_REQUEST));
    };
    let _ = upgrades.send(upgrade::on(&mut request));
    let mut response = empty_response(StatusCode::SWITCHING_PROTOCOLS);
    let headers = response.headers_mut();
    headers.insert(CONNECTION, "upgrade".parse().expect("static header"));
    headers.insert(UPGRADE, "websocket".parse().expect("static header"));
    headers.insert(SEC_WEBSOCKET_ACCEPT, accept.parse().expect("base64 header"));
    Ok(response)
}

pub fn empty_response(status: StatusCode) -> Response<String> {
    let mut response = Response::new(String::new());
    *response.status_mut() = status;
    response
}

/// Public node configuration: a generated Mostro key and the given relays.
pub fn set_node_env(command: &mut Command, mostro: &Keys, relays: &[&LoopbackRelay]) {
    let urls: Vec<&str> = relays.iter().map(|relay| relay.url.as_str()).collect();
    command
        .env("MOSTRO_PUBKEY", mostro.public_key().to_hex())
        .env("RELAYS", urls.join(","));
}

/// Streams and exit status of one child. `status` is `None` when the watchdog
/// killed it; `reaped` is whether the parent collected the process either way.
pub struct Captured {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub reaped: bool,
}

impl Captured {
    pub fn code(&self) -> Option<i32> {
        self.status.and_then(|status| status.code())
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Run a child to completion, or kill and reap it once `limit` elapses.
pub async fn run_with_watchdog(mut command: Command, limit: Duration) -> io::Result<Captured> {
    let mut child = command.spawn()?;
    // A piped stdin is closed at once; the child must never wait for input.
    drop(child.stdin.take());
    let stdout = read_all(child.stdout.take());
    let stderr = read_all(child.stderr.take());
    let status = match timeout(limit, child.wait()).await {
        Ok(status) => Some(status?),
        Err(_) => {
            // `kill` waits for the exit, so the process is reaped here.
            child.kill().await?;
            None
        }
    };
    let reaped = child.id().is_none();
    Ok(Captured {
        status,
        stdout: stdout.await.map_err(io::Error::other)??,
        stderr: stderr.await.map_err(io::Error::other)??,
        reaped,
    })
}

pub fn read_all<R>(pipe: Option<R>) -> JoinHandle<io::Result<Vec<u8>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes).await?;
        }
        Ok(bytes)
    })
}

pub fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

/// Text that must never be echoed back by a rejected invocation.
pub const SENTINEL: &str = "secret-sentinel-7f3a";

pub fn assert_no_effects(sandbox: &Sandbox, label: &str) {
    for dir in [sandbox.home(), sandbox.mcli_dir(), sandbox.cwd()] {
        assert!(is_empty_dir(&dir), "{label}: {dir:?} was written");
    }
}

/// The write end of a pipe whose read end is already closed, so any write by
/// the child fails with a broken pipe. Closing it before the spawn leaves no
/// window in which the child can write successfully.
pub fn closed_pipe() -> Stdio {
    let (reader, writer) = io::pipe().unwrap();
    drop(reader);
    Stdio::from(writer)
}

/// Run `order list` with exactly the given node environment and arguments.
pub async fn run_with_node_env(args: &[&str], env: &[(&str, &str)]) -> (Captured, Sandbox) {
    let sandbox = Sandbox::new().unwrap();
    let mut command = sandbox.command(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped && run.status.is_some(), "{args:?}");
    (run, sandbox)
}
