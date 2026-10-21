//! Process-level tests for the order-list pilot.
//!
//! Every child runs the real `mostro-cli` binary with an allow-listed
//! environment, sandboxed HOME/USERPROFILE/MCLI_DIR and working directory,
//! closed stdin, captured streams and a parent watchdog. Relays are in-process
//! nostr-sdk `LocalRelay`s behind a loopback listener on an OS-assigned port.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::ffi::OsStr;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, io, process};

use hyper::body::Incoming;
use hyper::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::{self, OnUpgrade};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use nostr_sdk::prelude::{Client, EventBuilder, Filter, FinalizeEvent, Keys, Kind, LocalRelay};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;
use tungstenite::handshake::derive_accept_key;

/// Parent-side limit for one child run.
const WATCHDOG: Duration = Duration::from_secs(40);
/// The only parent variables a child inherits; Windows needs SYSTEMROOT to
/// initialize sockets.
const INHERITED_ENV: &[&str] = &["SYSTEMROOT"];
/// Variables pointing every identity location into the sandbox.
const SANDBOX_ENV: &[&str] = &["HOME", "USERPROFILE", "MCLI_DIR"];

static SANDBOX_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Per-test directories, removed on drop, including when an assertion panics.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> io::Result<Self> {
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

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn mcli_dir(&self) -> PathBuf {
        self.root.join("mcli")
    }

    fn cwd(&self) -> PathBuf {
        self.root.join("cwd")
    }

    /// A `mostro-cli` command that sees only the allow-listed and sandbox
    /// environment. Node configuration is added separately by the test.
    fn command<I, S>(&self, args: I) -> Command
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
enum Backend {
    /// Serve the Nostr protocol from this relay.
    Relay(Box<LocalRelay>),
    /// Complete the upgrade, then never answer.
    Silent,
}

/// Loopback WebSocket endpoint on an OS-assigned port. The socket is bound and
/// listening when `start` returns, so a child can connect without waiting.
struct LoopbackRelay {
    url: String,
    connections: Arc<AtomicUsize>,
    backend: Backend,
    accept: JoinHandle<()>,
}

impl LoopbackRelay {
    async fn start(backend: Backend) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let url = format!("ws://{}", listener.local_addr()?);
        let connections = Arc::new(AtomicUsize::new(0));
        let accept = tokio::spawn(accept_loop(
            listener,
            backend.clone(),
            Arc::clone(&connections),
        ));
        Ok(Self {
            url,
            connections,
            backend,
            accept,
        })
    }

    /// TCP connections accepted so far.
    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
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

async fn accept_loop(listener: TcpListener, backend: Backend, connections: Arc<AtomicUsize>) {
    let mut sessions = JoinSet::new();
    while let Ok((stream, remote)) = listener.accept().await {
        connections.fetch_add(1, Ordering::SeqCst);
        sessions.spawn(session(stream, remote, backend.clone()));
    }
}

async fn session(stream: TcpStream, remote: SocketAddr, backend: Backend) {
    let (upgrades, mut pending) = unbounded_channel();
    let service = service_fn(move |request| handshake(request, upgrades.clone()));
    let served = http1::Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades()
        .await;
    if served.is_err() {
        return;
    }
    let Some(on_upgrade) = pending.recv().await else {
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
            std::future::pending::<()>().await;
        }
    }
}

async fn handshake(
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

fn empty_response(status: StatusCode) -> Response<String> {
    let mut response = Response::new(String::new());
    *response.status_mut() = status;
    response
}

/// Public node configuration: a generated Mostro key and the given relays.
fn set_node_env(command: &mut Command, mostro: &Keys, relays: &[&LoopbackRelay]) {
    let urls: Vec<&str> = relays.iter().map(|relay| relay.url.as_str()).collect();
    command
        .env("MOSTRO_PUBKEY", mostro.public_key().to_hex())
        .env("RELAYS", urls.join(","));
}

/// Streams and exit status of one child. `status` is `None` when the watchdog
/// killed it; `reaped` is whether the parent collected the process either way.
struct Captured {
    status: Option<ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    reaped: bool,
}

impl Captured {
    fn code(&self) -> Option<i32> {
        self.status.and_then(|status| status.code())
    }

    fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Run a child to completion, or kill and reap it once `limit` elapses.
async fn run_with_watchdog(mut command: Command, limit: Duration) -> io::Result<Captured> {
    let mut child = command.spawn()?;
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

fn read_all<R>(pipe: Option<R>) -> JoinHandle<io::Result<Vec<u8>>>
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

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

#[test]
fn child_environment_is_allow_listed_and_sandboxed() {
    let sandbox = Sandbox::new().unwrap();
    let command = sandbox.command(["--help"]);
    let command = command.as_std();

    let envs: BTreeMap<String, Option<PathBuf>> = command
        .get_envs()
        .map(|(key, value)| (key.to_string_lossy().into_owned(), value.map(PathBuf::from)))
        .collect();
    let allowed: BTreeSet<&str> = SANDBOX_ENV.iter().chain(INHERITED_ENV).copied().collect();
    assert!(
        envs.keys().all(|key| allowed.contains(key.as_str())),
        "unexpected child variables: {envs:?}"
    );
    for key in SANDBOX_ENV {
        let value = envs[*key].as_deref().expect("sandbox variable is set");
        assert!(value.starts_with(&sandbox.root), "{key}={value:?}");
    }
    assert_eq!(command.get_current_dir(), Some(sandbox.cwd().as_path()));
}

#[test]
fn sandbox_is_removed_on_drop() {
    let root = {
        let sandbox = Sandbox::new().unwrap();
        fs::write(sandbox.mcli_dir().join("probe"), b"x").unwrap();
        sandbox.root.clone()
    };
    assert!(!root.exists(), "{root:?} survived the sandbox");
}

#[tokio::test]
async fn parent_config_does_not_leak_into_the_child() {
    let sandbox = Sandbox::new().unwrap();

    let run = run_with_watchdog(sandbox.command(["listorders"]), WATCHDOG)
        .await
        .unwrap();

    assert!(run.reaped);
    assert_eq!(run.code(), Some(1), "stderr: {}", run.stderr_text());
    assert!(run.stdout.is_empty(), "stdout: {}", run.stdout_text());
    assert!(run.stderr_text().contains("MOSTRO_PUBKEY not set"));
    for dir in [sandbox.home(), sandbox.mcli_dir(), sandbox.cwd()] {
        assert!(is_empty_dir(&dir), "{dir:?} was written");
    }
}

#[tokio::test]
async fn loopback_relay_serves_sdk_clients_on_an_os_assigned_port() {
    let relay = LocalRelay::new();
    let seeded = EventBuilder::new(Kind::TextNote, "harness")
        .finalize(&Keys::generate())
        .unwrap();
    relay.add_event(seeded.clone()).await.unwrap();
    let endpoint = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();

    let port: u16 = endpoint
        .url
        .strip_prefix("ws://127.0.0.1:")
        .expect("loopback URL")
        .parse()
        .unwrap();
    assert_ne!(port, 0);

    let client = Client::default();
    client.add_relay(endpoint.url.as_str()).await.unwrap();
    client.connect().and_wait(Duration::from_secs(5)).await;
    let events = client
        .fetch_events(Filter::new().kind(Kind::TextNote))
        .timeout(Duration::from_secs(5))
        .await
        .unwrap();

    assert_eq!(
        events.into_iter().map(|event| event.id).collect::<Vec<_>>(),
        vec![seeded.id]
    );
    assert_eq!(endpoint.connections(), 1);
}

#[tokio::test]
async fn child_reaches_the_loopback_relay_with_sandboxed_storage() {
    let sandbox = Sandbox::new().unwrap();
    let endpoint = LoopbackRelay::start(Backend::Relay(Box::default()))
        .await
        .unwrap();
    let mut command = sandbox.command(["listorders"]);
    set_node_env(&mut command, &Keys::generate(), &[&endpoint]);

    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();

    assert!(run.reaped);
    assert_eq!(run.code(), Some(0), "stderr: {}", run.stderr_text());
    assert!(run.stdout_text().contains("List Orders"));
    assert!(endpoint.connections() >= 1);
    assert!(is_empty_dir(&sandbox.home()), "HOME was written");
}

#[tokio::test]
async fn watchdog_kills_and_reaps_a_hung_child() {
    let sandbox = Sandbox::new().unwrap();
    let endpoint = LoopbackRelay::start(Backend::Silent).await.unwrap();
    let mut command = sandbox.command(["listorders"]);
    set_node_env(&mut command, &Keys::generate(), &[&endpoint]);
    let limit = Duration::from_secs(2);

    let started = Instant::now();
    let run = run_with_watchdog(command, limit).await.unwrap();

    assert!(run.status.is_none(), "child exited: {:?}", run.status);
    assert!(run.reaped);
    assert!(endpoint.connections() >= 1);
    assert!(started.elapsed() < limit + Duration::from_secs(5));
}
