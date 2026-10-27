//! Process-level tests for the order-list pilot.
//!
//! Every child runs the real `mostro-cli` binary with an allow-listed
//! environment, sandboxed HOME/USERPROFILE/MCLI_DIR and working directory,
//! closed stdin, captured streams and a parent watchdog. Relays are in-process
//! nostr-sdk `LocalRelay`s behind a loopback listener on an OS-assigned port.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::ffi::OsStr;
use std::future::{pending, Future};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{ExitStatus, Stdio};
use std::slice::from_ref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, io, process};

use chrono::{DateTime, Utc};
use futures::future::join_all;
use futures::{Stream, StreamExt};
use hyper::body::Incoming;
use hyper::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::{self, OnUpgrade};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use mostro_client::commands::order::list::{list_orders, COMMAND};
use mostro_client::msg::{fetch_msg, FetchFailure, PublicReadConfig, ReadLimits};
use mostro_client::outcome::order::Snapshot;
use mostro_client::outcome::ErrorCode;
use mostro_client::util::events::create_seven_days_filter_at;
use mostro_core::prelude::NOSTR_ORDER_EVENT_KIND;
use nostr_memory::MemoryDatabase;
use nostr_sdk::prelude::{
    Client, Event, EventBuilder, EventId, Filter, FinalizeEvent, Keys, Kind, LocalRelay,
    MachineReadablePrefix, NostrDatabase, QueryPolicy, QueryPolicyResult, RelayMessage,
    RelayNotification, RelayStatus, SubscriptionId, Tag, Timestamp,
};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Notify;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, timeout};
use tungstenite::handshake::derive_accept_key;
use uuid::Uuid;

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
    open: Arc<AtomicUsize>,
    ended: Arc<Notify>,
    backend: Backend,
    accept: JoinHandle<()>,
}

impl LoopbackRelay {
    async fn start(backend: Backend) -> io::Result<Self> {
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
    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Whether every accepted connection ended within `limit`. A `Silent`
    /// session never notices its peer closing, so use this on relays only.
    async fn wait_idle(&self, limit: Duration) -> bool {
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
struct Sessions {
    accepted: Arc<AtomicUsize>,
    open: Arc<AtomicUsize>,
    ended: Arc<Notify>,
}

async fn accept_loop(listener: TcpListener, backend: Backend, counters: Sessions) {
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

async fn session(stream: TcpStream, remote: SocketAddr, backend: Backend) {
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

/// How long a test waits for one relay's answer to a subscription.
const OBSERVE: Duration = Duration::from_secs(2);

/// Relay behaviours a read must tell apart.
#[derive(Clone, Copy, Debug)]
enum Fixture {
    /// Answers with EOSE and no events.
    Empty,
    /// Answers every REQ with CLOSED.
    Closed,
    /// Accepts the WebSocket, then never answers.
    Silent,
    /// Receives the REQ, then the connection drops before EOSE.
    Disconnect,
}

#[derive(Debug)]
struct RejectQueries;

impl QueryPolicy for RejectQueries {
    fn admit_query<'a>(
        &'a self,
        _query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        Box::pin(async {
            QueryPolicyResult::reject(MachineReadablePrefix::Restricted, "queries disabled")
        })
    }
}

/// Signals that a REQ arrived and never answers it.
#[derive(Debug)]
struct HoldQueries {
    received: Arc<Notify>,
}

impl QueryPolicy for HoldQueries {
    fn admit_query<'a>(
        &'a self,
        _query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        self.received.notify_one();
        Box::pin(pending())
    }
}

/// The endpoint for `fixture` and the signal that it is holding a REQ, which
/// only `Disconnect` ever sends.
async fn start_fixture(fixture: Fixture) -> (LoopbackRelay, Arc<Notify>) {
    let held = Arc::new(Notify::new());
    let backend = match fixture {
        Fixture::Empty => Backend::Relay(Box::default()),
        Fixture::Closed => Backend::Relay(Box::new(
            LocalRelay::builder().query_policy(RejectQueries).build(),
        )),
        Fixture::Silent => Backend::Silent,
        Fixture::Disconnect => Backend::Relay(Box::new(
            LocalRelay::builder()
                .query_policy(HoldQueries {
                    received: Arc::clone(&held),
                })
                .build(),
        )),
    };
    (LoopbackRelay::start(backend).await.unwrap(), held)
}

async fn seeded_endpoint(event: &Event) -> LoopbackRelay {
    let relay = LocalRelay::new();
    relay.add_event(event.clone()).await.unwrap();
    LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap()
}

/// Run `work` while `endpoint` serves; for `Disconnect`, drop the endpoint,
/// and with it every connection, as soon as it holds a REQ.
async fn run_against<F: Future>(
    fixture: Fixture,
    endpoint: LoopbackRelay,
    held: Arc<Notify>,
    work: F,
) -> F::Output {
    let serve = async move {
        if matches!(fixture, Fixture::Disconnect) {
            held.notified().await;
            drop(endpoint);
            None
        } else {
            Some(endpoint)
        }
    };
    let (output, _serving) = tokio::join!(work, serve);
    output
}

async fn connected_client(urls: &[&str]) -> Client {
    let client = Client::default();
    for url in urls {
        client.add_relay(*url).await.unwrap();
    }
    let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
    assert!(connected.failed.is_empty(), "{:?}", connected.failed);
    client
}

/// What one relay did with one subscription, judged from its own
/// notification stream.
#[derive(Debug, PartialEq, Eq)]
enum Completion {
    /// EOSE, with the IDs of the events this relay sent before it.
    Eose(BTreeSet<EventId>),
    /// CLOSED, with the relay's message.
    Closed(String),
    /// The connection ended before a terminal message.
    Disconnected,
    /// Nothing terminal within `OBSERVE`.
    NoSignal,
}

async fn completion<S>(mut notifications: S, id: &SubscriptionId) -> Completion
where
    S: Stream<Item = RelayNotification> + Unpin,
{
    let watch = async {
        let mut events = BTreeSet::new();
        while let Some(notification) = notifications.next().await {
            match notification {
                // `RelayNotification::Event` is sent only for events new to
                // the client, so a second relay's copy would be missed.
                RelayNotification::Message { message } => match *message {
                    RelayMessage::Event {
                        subscription_id,
                        event,
                    } if *subscription_id == *id => {
                        events.insert(event.id);
                    }
                    RelayMessage::EndOfStoredEvents(subscription_id) if *subscription_id == *id => {
                        return Completion::Eose(events);
                    }
                    RelayMessage::Closed {
                        subscription_id,
                        message,
                    } if *subscription_id == *id => {
                        return Completion::Closed(message.into_owned());
                    }
                    _ => {}
                },
                RelayNotification::RelayStatus {
                    status:
                        RelayStatus::Disconnected | RelayStatus::Terminated | RelayStatus::Shutdown,
                } => return Completion::Disconnected,
                _ => {}
            }
        }
        Completion::Disconnected
    };
    timeout(OBSERVE, watch)
        .await
        .unwrap_or(Completion::NoSignal)
}

/// Relays a subscription was sent to, and each relay's completion in `urls`
/// order. Every relay's stream is armed before the REQ goes out.
async fn subscribe_and_observe(
    client: &Client,
    urls: &[&str],
    id: &SubscriptionId,
) -> (usize, Vec<Completion>) {
    let mut armed = Vec::new();
    for url in urls {
        let relay = client.relay(*url).await.unwrap().expect("relay was added");
        armed.push(relay.notifications());
    }
    let output = client
        .subscribe(Filter::new().kind(Kind::TextNote))
        .with_id(id.clone())
        .await
        .unwrap();
    assert!(output.failed.is_empty(), "{:?}", output.failed);
    let completions = join_all(armed.into_iter().map(|stream| completion(stream, id))).await;
    let _ = client.unsubscribe(id).await;
    (output.success.len(), completions)
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

    let run = run_with_watchdog(sandbox.command(["order", "list"]), WATCHDOG)
        .await
        .unwrap();

    assert!(run.reaped);
    assert_eq!(run.code(), Some(1), "stderr: {}", run.stderr_text());
    assert!(run.stdout.is_empty(), "stdout: {}", run.stdout_text());
    assert_eq!(
        run.stderr_text(),
        "error: Required node configuration is missing.\n"
    );
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
    let mut command = sandbox.command(["order", "list"]);
    set_node_env(&mut command, &Keys::generate(), &[&endpoint]);

    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();

    assert!(run.reaped);
    assert_eq!(run.code(), Some(0), "stderr: {}", run.stderr_text());
    assert!(run.stdout_text().contains("List Orders"));
    assert!(run.stdout_text().contains("No offers found"));
    assert!(endpoint.connections() >= 1);
    for dir in [sandbox.home(), sandbox.mcli_dir(), sandbox.cwd()] {
        assert!(is_empty_dir(&dir), "{dir:?} was written");
    }
}

#[tokio::test]
async fn watchdog_kills_and_reaps_a_hung_child() {
    let sandbox = Sandbox::new().unwrap();
    let endpoint = LoopbackRelay::start(Backend::Silent).await.unwrap();
    let mut command = sandbox.command(["order", "list"]);
    set_node_env(&mut command, &Keys::generate(), &[&endpoint]);
    let limit = Duration::from_secs(2);

    let started = Instant::now();
    let run = run_with_watchdog(command, limit).await.unwrap();

    assert!(run.status.is_none(), "child exited: {:?}", run.status);
    assert!(run.reaped);
    assert!(endpoint.connections() >= 1);
    assert!(
        started.elapsed() < limit + Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn convenience_fetch_reports_success_for_every_failure() {
    for fixture in [
        Fixture::Empty,
        Fixture::Closed,
        Fixture::Silent,
        Fixture::Disconnect,
    ] {
        let (endpoint, held) = start_fixture(fixture).await;
        let client = connected_client(&[endpoint.url.as_str()]).await;

        let fetch = async {
            client
                .fetch_events(Filter::new().kind(Kind::TextNote))
                .timeout(OBSERVE)
                .await
        };
        let fetched = run_against(fixture, endpoint, held, fetch).await;

        assert_eq!(
            fetched.map(|events| events.len()).ok(),
            Some(0),
            "{fixture:?}"
        );
        client.shutdown().await;
    }
}

#[tokio::test]
async fn explicit_subscription_distinguishes_completion_per_relay() {
    let seeded = EventBuilder::new(Kind::TextNote, "order")
        .finalize(&Keys::generate())
        .unwrap();
    let endpoints = [
        seeded_endpoint(&seeded).await,
        seeded_endpoint(&seeded).await,
        start_fixture(Fixture::Empty).await.0,
        start_fixture(Fixture::Closed).await.0,
        start_fixture(Fixture::Silent).await.0,
    ];
    let urls: Vec<&str> = endpoints
        .iter()
        .map(|endpoint| endpoint.url.as_str())
        .collect();
    let client = connected_client(&urls).await;
    let id = SubscriptionId::new("order-list-probe");

    let (subscribed, completions) = subscribe_and_observe(&client, &urls, &id).await;

    // The REQ counts as sent everywhere, including where it never completes.
    assert_eq!(subscribed, urls.len());
    let seeded_ids = BTreeSet::from([seeded.id]);
    assert_eq!(completions[0], Completion::Eose(seeded_ids.clone()));
    assert_eq!(completions[1], Completion::Eose(seeded_ids));
    assert_eq!(completions[2], Completion::Eose(BTreeSet::new()));
    assert!(
        matches!(&completions[3], Completion::Closed(message) if message.contains("queries disabled")),
        "{:?}",
        completions[3]
    );
    assert_eq!(completions[4], Completion::NoSignal);
    client.shutdown().await;
}

#[tokio::test]
async fn disconnect_before_eose_is_observed_per_relay() {
    let (endpoint, held) = start_fixture(Fixture::Disconnect).await;
    let url = endpoint.url.clone();
    let urls = [url.as_str()];
    let client = connected_client(&urls).await;
    let id = SubscriptionId::new("order-list-probe");

    let observe = subscribe_and_observe(&client, &urls, &id);
    let (subscribed, completions) = run_against(Fixture::Disconnect, endpoint, held, observe).await;

    assert_eq!(subscribed, 1);
    assert_eq!(completions, vec![Completion::Disconnected]);
    client.shutdown().await;
}

/// Text that must never be echoed back by a rejected invocation.
const SENTINEL: &str = "secret-sentinel-7f3a";
const ORDER_ID: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";

/// Run `args` with node and daemon-RPC configuration pointing at `endpoint`,
/// so any network or storage effect of a rejected invocation is observable.
async fn run_configured(args: &[&str], endpoint: &LoopbackRelay) -> (Captured, Sandbox) {
    let sandbox = Sandbox::new().unwrap();
    let mut command = sandbox.command(args);
    set_node_env(&mut command, &Keys::generate(), &[endpoint]);
    let rpc_url = endpoint.url.replacen("ws://", "http://", 1);
    command
        .env("MOSTRO_RPC_URL", rpc_url)
        .env("MOSTRO_RPC_TOKEN", SENTINEL);
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    (run, sandbox)
}

fn assert_no_effects(sandbox: &Sandbox, label: &str) {
    for dir in [sandbox.home(), sandbox.mcli_dir(), sandbox.cwd()] {
        assert!(is_empty_dir(&dir), "{label}: {dir:?} was written");
    }
}

#[tokio::test]
async fn argument_errors_exit_2_with_safe_stderr_before_any_effect() {
    let endpoint = LoopbackRelay::start(Backend::Relay(Box::default()))
        .await
        .unwrap();
    let unknown_flag = format!("--{SENTINEL}");
    let rows: [&[&str]; 6] = [
        &["order", "list", &unknown_flag],
        &[SENTINEL],
        &["--output", SENTINEL, "order", "list"],
        &["--output", "json", "--output", "json", "order", "list"],
        &["--output", "human", "order", "list", &unknown_flag],
        &["listorders"],
    ];
    for args in rows {
        let label = format!("{args:?}");
        let (run, sandbox) = run_configured(args, &endpoint).await;
        let stderr = run.stderr_text();
        assert_eq!(run.code(), Some(2), "{label}: {stderr}");
        assert!(run.stdout.is_empty(), "{label}: {}", run.stdout_text());
        assert!(stderr.starts_with("error: "), "{label}: {stderr}");
        assert!(!stderr.contains(SENTINEL), "{label}: {stderr}");
        assert_no_effects(&sandbox, &label);
    }
    assert_eq!(endpoint.connections(), 0);
}

/// Run `args` with the stderr reader already gone, so every diagnostic write
/// fails with a broken pipe, and return the status and stdout.
async fn run_with_broken_stderr(args: &[&str]) -> (Option<i32>, String) {
    let sandbox = Sandbox::new().unwrap();
    let mut child = sandbox.command(args).spawn().unwrap();
    drop(child.stdin.take());
    drop(child.stderr.take());
    let stdout = read_all(child.stdout.take());
    let status = timeout(WATCHDOG, child.wait()).await.unwrap().unwrap();
    let stdout = stdout.await.unwrap().unwrap();
    (status.code(), String::from_utf8_lossy(&stdout).into_owned())
}

#[tokio::test]
async fn failed_diagnostic_write_keeps_the_exit_status_and_stdout_empty() {
    // Human mode reports through stderr only; an unwritable stderr must not
    // turn the controlled exit into a panic (101) or change stdout.
    let rows: [(&[&str], i32); 3] = [
        (&["order", "list", "--side", "invalid"], 2),
        (&["--output", "human", "order", "list", "--bogus"], 2),
        // No node configuration in the sandbox: configuration_missing.
        (&["order", "list"], 1),
    ];
    for (args, expected) in rows {
        let (code, stdout) = run_with_broken_stderr(args).await;
        assert_eq!(code, Some(expected), "{args:?}");
        assert!(stdout.is_empty(), "{args:?}: {stdout}");
    }
}

#[tokio::test]
async fn json_intent_gets_one_error_envelope_before_any_effect() {
    let endpoint = LoopbackRelay::start(Backend::Relay(Box::default()))
        .await
        .unwrap();
    let unknown_flag = format!("--{SENTINEL}");
    let rows: [(&[&str], Value, &str); 9] = [
        (
            &["--output", "json", "listdisputes"],
            json!("dispute.list"),
            "output_not_supported",
        ),
        (
            &["listdisputes", "--output=json"],
            json!("dispute.list"),
            "output_not_supported",
        ),
        (
            &["--output", "json", "listorders"],
            Value::Null,
            "invalid_arguments",
        ),
        (
            &["--output", "json", "getlasttradeprivkey"],
            json!("account.trade-key"),
            "output_not_supported",
        ),
        (
            &["--output", "json", SENTINEL],
            Value::Null,
            "invalid_arguments",
        ),
        (
            &["--output", "json", "neworder", &unknown_flag],
            json!("order.new"),
            "invalid_arguments",
        ),
        (&["--output", "json"], Value::Null, "invalid_arguments"),
        (
            &["--output", "json", "admmaintenancestatus"],
            json!("maintenance.status"),
            "output_not_supported",
        ),
        (
            &["admcancelpending", "--orderid", ORDER_ID, "--output=json"],
            json!("maintenance.cancel-pending"),
            "output_not_supported",
        ),
    ];
    for (args, command, code) in rows {
        let label = format!("{args:?}");
        let (run, sandbox) = run_configured(args, &endpoint).await;
        let stdout = run.stdout_text();
        assert_eq!(run.code(), Some(2), "{label}: {}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{label}: {}", run.stderr_text());
        assert!(
            stdout.ends_with('\n') && stdout.lines().count() == 1,
            "{label}: {stdout}"
        );
        assert!(!stdout.contains(SENTINEL), "{label}: {stdout}");
        let outcome: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(
            outcome,
            json!({
                "schema_version": "1",
                "command": command,
                "status": "error",
                "data": null,
                "error": {"code": code, "message": outcome["error"]["message"]},
                "warnings": [],
                "meta": {
                    "publication": "not_attempted",
                    "daemon_acceptance": "unknown",
                    "persistence": "not_attempted",
                    "snapshot": null,
                },
            }),
            "{label}"
        );
        assert_no_effects(&sandbox, &label);
    }
    assert_eq!(endpoint.connections(), 0);
}

/// `order list` owns its JSON: argument errors keep `order.list` with read
/// stages, and a valid invocation reads the configured relay.
#[tokio::test]
async fn order_list_json_uses_read_stages() {
    let endpoint = LoopbackRelay::start(Backend::Relay(Box::default()))
        .await
        .unwrap();
    let rows: [(&[&str], i32, &str); 3] = [
        (
            &["--output", "json", "order", "list", "--side", "both"],
            2,
            "invalid_arguments",
        ),
        (
            &["order", "--output=json", "list", "--kind", "sell"],
            2,
            "invalid_arguments",
        ),
        (&["order", "list", "--output", "json"], 0, "ok"),
    ];
    for (args, exit, code) in rows {
        let label = format!("{args:?}");
        let (run, sandbox) = run_configured(args, &endpoint).await;
        assert_eq!(run.code(), Some(exit), "{label}: {}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{label}: {}", run.stderr_text());
        let stdout = run.stdout_text();
        assert_eq!(stdout.lines().count(), 1, "{label}: {stdout}");
        let outcome: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(outcome["command"], "order.list", "{label}");
        let status = outcome["error"]["code"].as_str().unwrap_or("ok");
        assert_eq!(status, code, "{label}: {outcome}");
        for stage in ["publication", "daemon_acceptance", "persistence"] {
            assert_eq!(outcome["meta"][stage], "not_applicable", "{label}");
        }
        assert_no_effects(&sandbox, &label);
    }
    assert_eq!(endpoint.connections(), 1);
}

/// Positive control: the same configuration does reach the endpoint when an
/// RPC command runs in human mode, so zero connections above is meaningful.
#[tokio::test]
async fn human_rpc_command_reaches_the_configured_daemon() {
    let endpoint = LoopbackRelay::start(Backend::Silent).await.unwrap();
    let (run, _sandbox) = run_configured(&["admmaintenancestatus"], &endpoint).await;
    assert_ne!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(endpoint.connections() >= 1);
}

#[tokio::test]
async fn help_and_version_stay_text_on_stdout() {
    let endpoint = LoopbackRelay::start(Backend::Silent).await.unwrap();
    let rows: [&[&str]; 5] = [
        &["--help"],
        &["--output", "json", "--help"],
        &["order", "list", "--output", "json", "--help"],
        &["--version"],
        &["--output", "json", "--version"],
    ];
    for args in rows {
        let label = format!("{args:?}");
        let (run, sandbox) = run_configured(args, &endpoint).await;
        let stdout = run.stdout_text();
        assert_eq!(run.code(), Some(0), "{label}: {}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{label}: {}", run.stderr_text());
        assert!(stdout.contains("mostro-cli"), "{label}: {stdout}");
        assert!(!stdout.starts_with('{'), "{label}: {stdout}");
        assert_no_effects(&sandbox, &label);
    }
    assert_eq!(endpoint.connections(), 0);
}

#[tokio::test]
async fn bare_invocation_still_prints_help_on_stderr() {
    let endpoint = LoopbackRelay::start(Backend::Silent).await.unwrap();
    let (run, sandbox) = run_configured(&[], &endpoint).await;
    assert_eq!(run.code(), Some(2), "{}", run.stderr_text());
    assert!(run.stdout.is_empty(), "{}", run.stdout_text());
    assert!(
        run.stderr_text().contains("Usage:"),
        "{}",
        run.stderr_text()
    );
    assert_no_effects(&sandbox, "bare");
    assert_eq!(endpoint.connections(), 0);
}

// Collector contract (T021), implemented by `fetch_msg` in T026.

/// The fixed query clock, 2026-10-16T00:00:00Z, and its seven-day `since`.
const QUERY_AT: u64 = 1_792_108_800;
const QUERY_SINCE: &str = "2026-10-09T00:00:00Z";
const HOUR: u64 = 3_600;
const PRODUCTION_CAP: usize = 10_000;

/// The order-book filter the list command sends, at the fixed clock.
fn order_filter(mostro: &Keys) -> Filter {
    create_seven_days_filter_at(
        "order".to_string(),
        mostro.public_key(),
        NOSTR_ORDER_EVENT_KIND,
        query_clock(),
    )
    .unwrap()
}

fn query_clock() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(i64::try_from(QUERY_AT).unwrap(), 0).unwrap()
}

fn book_event(author: &Keys, kind: Kind, z: &str, created_at: u64) -> Event {
    EventBuilder::new(kind, "")
        .tags([
            Tag::custom("d", [Uuid::new_v4().to_string()]),
            Tag::custom("z", [z]),
        ])
        .custom_created_at(Timestamp::from(created_at))
        .finalize(author)
        .unwrap()
}

/// A matching public order event `hours` before the query time.
fn order_event(mostro: &Keys, hours: u64) -> Event {
    book_event(
        mostro,
        Kind::Custom(NOSTR_ORDER_EVENT_KIND),
        "order",
        QUERY_AT - hours * HOUR,
    )
}

/// `event` with its content changed after signing, so it no longer verifies.
fn forged(event: &Event) -> Event {
    let mut json: Value = serde_json::from_str(&event.as_json()).unwrap();
    json["content"] = json!("forged");
    let forged = Event::from_json(json.to_string()).unwrap();
    assert!(forged.verify().is_err());
    forged
}

async fn serve(relay: LocalRelay, events: &[Event]) -> LoopbackRelay {
    for event in events {
        relay.add_event(event.clone()).await.unwrap();
    }
    LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap()
}

fn read_config(mostro: &Keys, relays: &[&str]) -> PublicReadConfig {
    PublicReadConfig::new(mostro.public_key(), &relays.join(",")).unwrap()
}

fn quick_limits() -> ReadLimits {
    ReadLimits {
        connect: Duration::from_secs(2),
        read: Duration::from_secs(2),
        cleanup: Duration::from_secs(1),
        ..ReadLimits::default()
    }
}

fn ids<'a>(events: impl IntoIterator<Item = &'a Event>) -> BTreeSet<EventId> {
    events.into_iter().map(|event| event.id).collect()
}

/// The snapshot as JSON, after checking the fields that do not vary.
fn snapshot_json(snapshot: &Snapshot, cap: usize) -> Value {
    let json = serde_json::to_value(snapshot).unwrap();
    assert_eq!(json["since"], QUERY_SINCE, "{json}");
    assert_eq!(json["until"], Value::Null, "{json}");
    assert_eq!(json["per_relay_limit"], 50, "{json}");
    assert_eq!(json["max_unique_events"], cap, "{json}");
    assert_eq!(json["completion_policy"], "all_relays_eose", "{json}");
    assert_eq!(json["complete_history"], false, "{json}");
    let observed_at = json["observed_at"].as_str().expect("observed_at string");
    assert!(observed_at.ends_with('Z'), "{json}");
    assert!(DateTime::parse_from_rfc3339(observed_at).is_ok(), "{json}");
    json
}

fn assert_counts(json: &Value, configured: usize, completed: usize, received: usize) {
    assert_eq!(json["configured_relays"], configured, "{json}");
    assert_eq!(json["completed_relays"], completed, "{json}");
    assert_eq!(json["received_events"], received, "{json}");
}

/// The failure's snapshot JSON; a query that started always has one.
fn failure_snapshot(failure: &FetchFailure, cap: usize) -> Value {
    snapshot_json(failure.snapshot().expect("query started"), cap)
}

/// Records every REQ filter it admits.
#[derive(Debug, Default)]
struct RecordQueries {
    seen: Arc<Mutex<Vec<Filter>>>,
}

impl QueryPolicy for RecordQueries {
    fn admit_query<'a>(
        &'a self,
        query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        self.seen.lock().unwrap().push(query.clone());
        Box::pin(async { QueryPolicyResult::Accept })
    }
}

/// Answers every REQ with its whole store, ignoring the requested filter.
#[derive(Debug)]
struct IgnoreFilter;

impl QueryPolicy for IgnoreFilter {
    fn admit_query<'a>(
        &'a self,
        query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        *query = Filter::new();
        Box::pin(async { QueryPolicyResult::Accept })
    }
}

/// Signals each REQ, then answers it only once released.
#[derive(Clone, Debug, Default)]
struct Gate {
    received: Arc<Notify>,
    release: Arc<Notify>,
}

impl QueryPolicy for Gate {
    fn admit_query<'a>(
        &'a self,
        _query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        self.received.notify_one();
        Box::pin(async {
            self.release.notified().await;
            QueryPolicyResult::Accept
        })
    }
}

#[tokio::test]
async fn collector_sends_the_requested_filter_once_and_returns_its_events() {
    let mostro = Keys::generate();
    let events = [order_event(&mostro, 1), order_event(&mostro, 2)];
    let recorder = RecordQueries::default();
    let seen = Arc::clone(&recorder.seen);
    let relay = serve(
        LocalRelay::builder().query_policy(recorder).build(),
        &events,
    )
    .await;
    let filter = order_filter(&mostro);

    let fetched = fetch_msg(
        &read_config(&mostro, &[&relay.url]),
        filter.clone(),
        &quick_limits(),
    )
    .await
    .unwrap();

    assert_eq!(*seen.lock().unwrap(), vec![filter]);
    assert_eq!(ids(fetched.events()), ids(&events));
    assert!(fetched.warnings().is_empty());
    let snapshot = snapshot_json(fetched.snapshot(), PRODUCTION_CAP);
    assert_counts(&snapshot, 1, 1, 2);
    assert_eq!(relay.connections(), 1);
}

#[tokio::test]
async fn events_failing_verification_or_the_filter_are_dropped() {
    let mostro = Keys::generate();
    let order_kind = Kind::Custom(NOSTR_ORDER_EVENT_KIND);
    let valid = order_event(&mostro, 1);
    let unrelated = [
        forged(&order_event(&mostro, 2)),
        order_event(&Keys::generate(), 3),
        book_event(&mostro, Kind::TextNote, "order", QUERY_AT - HOUR),
        book_event(&mostro, order_kind, "dispute", QUERY_AT - HOUR),
        book_event(&mostro, order_kind, "order", QUERY_AT - 8 * 24 * HOUR),
    ];
    let database = MemoryDatabase::unbounded();
    for event in unrelated.iter().chain([&valid]) {
        database.save_event(event).await.unwrap();
    }
    let relay = LocalRelay::builder()
        .database(database)
        .query_policy(IgnoreFilter)
        .build();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();

    let fetched = fetch_msg(
        &read_config(&mostro, &[&relay.url]),
        order_filter(&mostro),
        &quick_limits(),
    )
    .await
    .unwrap();

    assert_eq!(ids(fetched.events()), BTreeSet::from([valid.id]));
    let snapshot = snapshot_json(fetched.snapshot(), PRODUCTION_CAP);
    assert_counts(&snapshot, 1, 1, 1);
}

#[tokio::test]
async fn duplicate_relays_and_events_count_once() {
    let mostro = Keys::generate();
    let shared = order_event(&mostro, 1);
    let only_first = order_event(&mostro, 2);
    let first = serve(LocalRelay::new(), &[shared.clone(), only_first.clone()]).await;
    let second = serve(LocalRelay::new(), from_ref(&shared)).await;
    let relays = format!(
        " {first} ,{first},,{second}",
        first = first.url,
        second = second.url
    );
    let config = PublicReadConfig::new(mostro.public_key(), &relays).unwrap();
    assert_eq!(config.relays().len(), 2);

    let fetched = fetch_msg(&config, order_filter(&mostro), &quick_limits())
        .await
        .unwrap();

    assert_eq!(ids(fetched.events()), ids([&shared, &only_first]));
    let snapshot = snapshot_json(fetched.snapshot(), PRODUCTION_CAP);
    assert_counts(&snapshot, 2, 2, 2);
    assert_eq!((first.connections(), second.connections()), (1, 1));
}

#[tokio::test]
async fn a_closed_relay_fails_the_whole_read() {
    let mostro = Keys::generate();
    let answering = serve(LocalRelay::new(), &[order_event(&mostro, 1)]).await;
    let (closed, _) = start_fixture(Fixture::Closed).await;

    let failure = fetch_msg(
        &read_config(&mostro, &[&answering.url, &closed.url]),
        order_filter(&mostro),
        &quick_limits(),
    )
    .await
    .unwrap_err();

    assert_eq!(failure.code(), ErrorCode::RelayReadFailed);
    let snapshot = failure_snapshot(&failure, PRODUCTION_CAP);
    assert_eq!(snapshot["configured_relays"], 2, "{snapshot}");
    // The read may fail before or after the answering relay's EOSE.
    assert!(
        snapshot["completed_relays"].as_u64().unwrap() <= 1,
        "{snapshot}"
    );
}

#[tokio::test]
async fn a_relay_disconnecting_before_eose_fails_the_read() {
    let mostro = Keys::generate();
    let answering = serve(LocalRelay::new(), &[order_event(&mostro, 1)]).await;
    let (dropping, held) = start_fixture(Fixture::Disconnect).await;
    let config = read_config(&mostro, &[&answering.url, &dropping.url]);
    let limits = quick_limits();

    let read = fetch_msg(&config, order_filter(&mostro), &limits);
    let failure = run_against(Fixture::Disconnect, dropping, held, read)
        .await
        .unwrap_err();

    assert_eq!(failure.code(), ErrorCode::RelayReadFailed);
    let snapshot = failure_snapshot(&failure, PRODUCTION_CAP);
    assert_eq!(snapshot["configured_relays"], 2, "{snapshot}");
    assert!(
        snapshot["completed_relays"].as_u64().unwrap() <= 1,
        "{snapshot}"
    );
}

#[tokio::test]
async fn a_silent_relay_times_out_and_cleanup_stays_bounded() {
    let mostro = Keys::generate();
    let answering = serve(LocalRelay::new(), &[order_event(&mostro, 1)]).await;
    let (silent, _) = start_fixture(Fixture::Silent).await;
    let limits = ReadLimits {
        read: Duration::from_millis(500),
        cleanup: Duration::from_millis(500),
        ..quick_limits()
    };

    let started = Instant::now();
    let failure = fetch_msg(
        &read_config(&mostro, &[&answering.url, &silent.url]),
        order_filter(&mostro),
        &limits,
    )
    .await
    .unwrap_err();
    let elapsed = started.elapsed();

    // Cleanup never replaces the original error.
    assert_eq!(failure.code(), ErrorCode::ReadTimeout);
    let snapshot = failure_snapshot(&failure, PRODUCTION_CAP);
    assert_counts(&snapshot, 2, 1, 1);
    assert!(
        elapsed >= limits.read && elapsed < limits.read + limits.cleanup + OBSERVE,
        "{elapsed:?}"
    );
    assert!(answering.wait_idle(limits.cleanup + OBSERVE).await);
}

#[tokio::test]
async fn an_unreachable_relay_fails_before_the_query() {
    let mostro = Keys::generate();
    let answering = serve(LocalRelay::new(), &[order_event(&mostro, 1)]).await;
    // Bound but never accepted: the WebSocket handshake can only time out.
    let unanswered = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let unanswered_url = format!("ws://{}", unanswered.local_addr().unwrap());
    let limits = ReadLimits {
        connect: Duration::from_millis(500),
        cleanup: Duration::from_millis(500),
        ..quick_limits()
    };

    let started = Instant::now();
    let failure = fetch_msg(
        &read_config(&mostro, &[&answering.url, &unanswered_url]),
        order_filter(&mostro),
        &limits,
    )
    .await
    .unwrap_err();
    let elapsed = started.elapsed();

    assert_eq!(failure.code(), ErrorCode::RelayConnectionFailed);
    assert!(failure.snapshot().is_none());
    assert!(
        elapsed < limits.connect + limits.cleanup + OBSERVE,
        "{elapsed:?}"
    );
    assert!(answering.wait_idle(limits.cleanup + OBSERVE).await);
}

#[tokio::test]
async fn events_after_a_relays_eose_are_ignored() {
    let mostro = Keys::generate();
    let stored = order_event(&mostro, 1);
    let late = order_event(&mostro, 2);
    let other = order_event(&mostro, 3);
    let (first_gate, second_gate) = (Gate::default(), Gate::default());
    let first_relay = LocalRelay::builder()
        .query_policy(first_gate.clone())
        .build();
    let first = serve(first_relay.clone(), from_ref(&stored)).await;
    let second = serve(
        LocalRelay::builder()
            .query_policy(second_gate.clone())
            .build(),
        from_ref(&other),
    )
    .await;
    let config = read_config(&mostro, &[&first.url, &second.url]);
    let limits = quick_limits();

    let read = fetch_msg(&config, order_filter(&mostro), &limits);
    let drive = async {
        first_gate.received.notified().await;
        // Never stored, so only the live subscription registered after the
        // first relay's EOSE can deliver it.
        assert!(first_relay.notify_event(late.clone()));
        first_gate.release.notify_one();
        second_gate.received.notified().await;
        // Lets the late event arrive before the read can finish; a shorter
        // wait only makes the test less sensitive, never wrong.
        sleep(Duration::from_millis(300)).await;
        second_gate.release.notify_one();
    };
    let (fetched, ()) = tokio::join!(read, drive);
    let fetched = fetched.unwrap();

    assert_eq!(ids(fetched.events()), ids([&stored, &other]));
    let snapshot = snapshot_json(fetched.snapshot(), PRODUCTION_CAP);
    assert_counts(&snapshot, 2, 2, 2);
}

#[tokio::test]
async fn a_relay_filling_its_notification_buffer_fails_the_read() {
    let mostro = Keys::generate();
    let events: Vec<Event> = (1..=20).map(|hours| order_event(&mostro, hours)).collect();
    let relay = serve(LocalRelay::new(), &events).await;
    let config = read_config(&mostro, &[&relay.url]);

    let fetched = fetch_msg(&config, order_filter(&mostro), &quick_limits())
        .await
        .unwrap();
    assert_eq!(fetched.events().len(), 20);

    let tiny = ReadLimits {
        notification_capacity: 8,
        ..quick_limits()
    };
    let failure = fetch_msg(&config, order_filter(&mostro), &tiny)
        .await
        .unwrap_err();

    assert_eq!(failure.code(), ErrorCode::RelayReadFailed);
    let snapshot = failure_snapshot(&failure, PRODUCTION_CAP);
    assert_eq!(snapshot["configured_relays"], 1, "{snapshot}");
    assert_eq!(snapshot["completed_relays"], 0, "{snapshot}");
}

#[tokio::test]
async fn exceeding_the_event_cap_fails_without_truncating() {
    let mostro = Keys::generate();
    let events: Vec<Event> = (1..=4).map(|hours| order_event(&mostro, hours)).collect();
    let relay = serve(LocalRelay::new(), &events).await;
    let config = read_config(&mostro, &[&relay.url]);
    let capped = |max_unique_events| ReadLimits {
        max_unique_events,
        ..quick_limits()
    };

    let fetched = fetch_msg(&config, order_filter(&mostro), &capped(4))
        .await
        .unwrap();
    assert_counts(&snapshot_json(fetched.snapshot(), 4), 1, 1, 4);

    let failure = fetch_msg(&config, order_filter(&mostro), &capped(3))
        .await
        .unwrap_err();

    assert_eq!(failure.code(), ErrorCode::ReadLimitExceeded);
    let snapshot = failure_snapshot(&failure, 3);
    assert_eq!(snapshot["received_events"], 3, "{snapshot}");
}

#[tokio::test]
async fn cancelling_a_read_closes_its_connections() {
    let mostro = Keys::generate();
    let answering = serve(LocalRelay::new(), &[order_event(&mostro, 1)]).await;
    let (holding, held) = start_fixture(Fixture::Disconnect).await;
    let config = read_config(&mostro, &[&answering.url, &holding.url]);
    let limits = quick_limits();

    tokio::select! {
        result = fetch_msg(&config, order_filter(&mostro), &limits) => {
            panic!("the read finished while a relay held its REQ: {result:?}")
        }
        () = held.notified() => {}
    }

    assert!(answering.wait_idle(OBSERVE).await);
}

// `order.list` operation (T028) over the collector and reducer.

/// A published order state `hours` before the query time.
fn listing(mostro: &Keys, id: u128, hours: u64, side: &str, status: &str) -> Event {
    listing_at(mostro, id, QUERY_AT - hours * HOUR, side, status)
}

fn listing_at(mostro: &Keys, id: u128, created_at: u64, side: &str, status: &str) -> Event {
    EventBuilder::new(Kind::Custom(NOSTR_ORDER_EVENT_KIND), "")
        .tags([
            Tag::custom("d", [Uuid::from_u128(id).to_string()]),
            Tag::custom("k", [side]),
            Tag::custom("s", [status]),
            Tag::custom("f", ["USD"]),
            Tag::custom("amt", ["0"]),
            Tag::custom("fa", ["100"]),
            Tag::custom("pm", ["cash"]),
            Tag::custom("premium", ["1"]),
            Tag::custom("z", ["order"]),
        ])
        .custom_created_at(Timestamp::from(created_at))
        .finalize(mostro)
        .unwrap()
}

async fn list(
    side: Option<&str>,
    config: impl FnOnce() -> Result<PublicReadConfig, ErrorCode>,
) -> (Value, i32) {
    let outcome = list_orders(side, None, None, config, &quick_limits(), query_clock()).await;
    (serde_json::to_value(&outcome).unwrap(), outcome.exit_code())
}

fn assert_read_stages(outcome: &Value) {
    assert_eq!(outcome["command"], COMMAND, "{outcome}");
    for stage in ["publication", "daemon_acceptance", "persistence"] {
        assert_eq!(outcome["meta"][stage], "not_applicable", "{outcome}");
    }
}

#[tokio::test]
async fn listing_selects_latest_matching_states_across_relays() {
    let mostro = Keys::generate();
    let first = serve(
        LocalRelay::new(),
        &[
            listing(&mostro, 1, 1, "sell", "pending"),
            listing(&mostro, 2, 3, "sell", "pending"),
            listing(&mostro, 3, 2, "buy", "pending"),
        ],
    )
    .await;
    let second = serve(
        LocalRelay::new(),
        &[
            listing(&mostro, 2, 2, "sell", "canceled"),
            listing(&mostro, 4, 4, "sell", "pending"),
        ],
    )
    .await;
    let config = read_config(&mostro, &[&first.url, &second.url]);

    let (outcome, exit) = list(Some("SELL"), || Ok(config)).await;

    assert_eq!(exit, 0, "{outcome}");
    assert_eq!(outcome["status"], "ok", "{outcome}");
    assert_eq!(outcome["error"], Value::Null, "{outcome}");
    assert_eq!(outcome["warnings"], json!([]), "{outcome}");
    assert_read_stages(&outcome);
    let data = &outcome["data"];
    assert_eq!(data["node_pubkey"], mostro.public_key().to_hex());
    assert_eq!(
        data["filters"],
        json!({ "side": "sell", "currency": null, "status": "pending" })
    );
    let rows: Vec<&str> = data["orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|order| order["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        rows,
        [
            Uuid::from_u128(1).to_string(),
            Uuid::from_u128(4).to_string()
        ]
    );
    let snapshot = &outcome["meta"]["snapshot"];
    assert_counts(snapshot, 2, 2, 5);
    assert_eq!(snapshot["since"], QUERY_SINCE, "{snapshot}");
}

#[tokio::test]
async fn listing_validates_filters_before_reading_configuration() {
    let (outcome, exit) = list(Some("both"), || panic!("configuration was read")).await;
    assert_eq!(exit, 2, "{outcome}");
    assert_eq!(outcome["error"]["code"], "invalid_arguments", "{outcome}");
    assert_eq!(outcome["data"], Value::Null, "{outcome}");
    assert_eq!(outcome["meta"]["snapshot"], Value::Null, "{outcome}");
    assert_read_stages(&outcome);

    let (outcome, exit) = list(None, || Err(ErrorCode::ConfigurationMissing)).await;
    assert_eq!(exit, 1, "{outcome}");
    assert_eq!(
        outcome["error"]["code"], "configuration_missing",
        "{outcome}"
    );
    assert_eq!(outcome["meta"]["snapshot"], Value::Null, "{outcome}");
    assert_read_stages(&outcome);
}

#[tokio::test]
async fn listing_failure_keeps_the_snapshot_and_returns_no_rows() {
    let mostro = Keys::generate();
    let answering = serve(
        LocalRelay::new(),
        &[listing(&mostro, 1, 1, "sell", "pending")],
    )
    .await;
    let (closed, _) = start_fixture(Fixture::Closed).await;
    let config = read_config(&mostro, &[&answering.url, &closed.url]);

    let (outcome, exit) = list(None, || Ok(config)).await;

    assert_eq!(exit, 1, "{outcome}");
    assert_eq!(outcome["status"], "error", "{outcome}");
    assert_eq!(outcome["error"]["code"], "relay_read_failed", "{outcome}");
    assert_eq!(outcome["data"], Value::Null, "{outcome}");
    assert_read_stages(&outcome);
    assert_eq!(
        outcome["meta"]["snapshot"]["configured_relays"], 2,
        "{outcome}"
    );
}

// `order list` binary matrix (T031): the real child against loopback relays,
// on the real clock.

/// The book every successful binary case reads: orders 1 and 2 are pending,
/// order 3 was pending but its latest state is canceled. Order events are
/// addressable, so the relay itself keeps only order 3's newest state.
fn live_book(mostro: &Keys) -> Vec<Event> {
    let ago = |hours: u64| Timestamp::now().as_secs() - hours * HOUR;
    vec![
        listing_at(mostro, 1, ago(1), "sell", "pending"),
        listing_at(mostro, 2, ago(2), "buy", "pending"),
        listing_at(mostro, 3, ago(3), "sell", "pending"),
        listing_at(mostro, 3, ago(2), "sell", "canceled"),
    ]
}

async fn run_listing(
    sandbox: &Sandbox,
    args: &[&str],
    mostro: &Keys,
    relays: &[&LoopbackRelay],
) -> Captured {
    let mut command = sandbox.command(args);
    set_node_env(&mut command, mostro, relays);
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped, "{args:?}: child was not reaped");
    assert!(run.status.is_some(), "{args:?}: watchdog fired");
    run
}

/// The one JSON document on stdout, fully consumed, whose status agrees with
/// the exit code.
fn one_document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert!(
        stdout.ends_with('\n') && stdout.lines().count() == 1,
        "{stdout}"
    );
    // `from_str` rejects anything after the document except whitespace.
    let outcome: Value = serde_json::from_str(&stdout).unwrap();
    assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    let exit = run.code().unwrap();
    let expected_status = if exit == 0 { "ok" } else { "error" };
    assert_eq!(outcome["status"], expected_status, "exit {exit}: {outcome}");
    assert_eq!(outcome["schema_version"], "1", "{outcome}");
    assert_read_stages(&outcome);
    outcome
}

fn row_ids(outcome: &Value) -> Vec<String> {
    outcome["data"]["orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|order| order["id"].as_str().unwrap().to_string())
        .collect()
}

/// Every event the relay stores; a listing must not add any.
async fn stored_ids(relay: &LoopbackRelay) -> BTreeSet<EventId> {
    let client = connected_client(&[relay.url.as_str()]).await;
    let events = client
        .fetch_events(Filter::new())
        .timeout(OBSERVE)
        .await
        .unwrap();
    client.shutdown().await;
    ids(events.iter())
}

#[tokio::test]
async fn order_list_binary_selects_rows_in_both_modes() {
    let mostro = Keys::generate();
    let book = live_book(&mostro);
    let relay = serve(LocalRelay::new(), &book).await;
    let stored = stored_ids(&relay).await;
    let sandbox = Sandbox::new().unwrap();

    let run = run_listing(
        &sandbox,
        &["--output", "json", "order", "list"],
        &mostro,
        &[&relay],
    )
    .await;
    assert_eq!(run.code(), Some(0));
    let outcome = one_document(&run);
    assert_eq!(
        row_ids(&outcome),
        [
            Uuid::from_u128(1).to_string(),
            Uuid::from_u128(2).to_string()
        ]
    );
    assert_eq!(outcome["data"]["node_pubkey"], mostro.public_key().to_hex());
    assert_eq!(outcome["warnings"], json!([]));
    assert_counts(&outcome["meta"]["snapshot"], 1, 1, stored.len());

    let run = run_listing(
        &sandbox,
        &["order", "list", "--side", "sell"],
        &mostro,
        &[&relay],
    )
    .await;
    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    let table = run.stdout_text();
    assert!(table.contains(&Uuid::from_u128(1).to_string()), "{table}");
    for absent in [2, 3] {
        assert!(
            !table.contains(&Uuid::from_u128(absent).to_string()),
            "{table}"
        );
    }

    assert_eq!(
        stored_ids(&relay).await,
        stored,
        "the listing published an event"
    );
    assert_no_effects(&sandbox, "fresh identity location");
}

#[tokio::test]
async fn order_list_binary_empty_results_are_successes() {
    let mostro = Keys::generate();
    let book = live_book(&mostro);
    let seeded = serve(LocalRelay::new(), &book).await;
    let stored = stored_ids(&seeded).await;
    let (empty, _) = start_fixture(Fixture::Empty).await;
    let rows: [(&[&str], &LoopbackRelay, usize); 2] = [
        (
            &["--output", "json", "order", "list", "--currency", "eur"],
            &seeded,
            stored.len(),
        ),
        (&["--output", "json", "order", "list"], &empty, 0),
    ];
    for (args, relay, received) in rows {
        let sandbox = Sandbox::new().unwrap();
        let run = run_listing(&sandbox, args, &mostro, &[relay]).await;
        assert_eq!(run.code(), Some(0), "{args:?}");
        let outcome = one_document(&run);
        assert_eq!(outcome["data"]["orders"], json!([]), "{args:?}");
        assert_counts(&outcome["meta"]["snapshot"], 1, 1, received);
        assert_no_effects(&sandbox, &format!("{args:?}"));
    }
    assert_eq!(stored_ids(&seeded).await, stored);
}

#[tokio::test]
async fn order_list_binary_read_failures_return_no_rows() {
    let mostro = Keys::generate();
    let answering = serve(LocalRelay::new(), &live_book(&mostro)).await;
    let (closed, _) = start_fixture(Fixture::Closed).await;
    let json = ["--output", "json", "order", "list"];
    let rows: [&[&LoopbackRelay]; 2] = [&[&closed], &[&answering, &closed]];
    for relays in rows {
        let sandbox = Sandbox::new().unwrap();
        let run = run_listing(&sandbox, &json, &mostro, relays).await;
        assert_eq!(run.code(), Some(1));
        let outcome = one_document(&run);
        assert_eq!(outcome["error"]["code"], "relay_read_failed", "{outcome}");
        assert_eq!(outcome["data"], Value::Null, "{outcome}");
        let snapshot = &outcome["meta"]["snapshot"];
        assert_eq!(snapshot["configured_relays"], relays.len(), "{outcome}");
        assert_no_effects(&sandbox, "read failure");
    }

    let sandbox = Sandbox::new().unwrap();
    let run = run_listing(
        &sandbox,
        &["order", "list"],
        &mostro,
        &[&answering, &closed],
    )
    .await;
    assert_eq!(run.code(), Some(1));
    assert!(run.stdout.is_empty(), "{}", run.stdout_text());
    assert_eq!(
        run.stderr_text(),
        "error: A configured relay failed during the public order read.\n"
    );
}

#[tokio::test]
async fn order_list_binary_times_out_on_a_silent_relay() {
    let mostro = Keys::generate();
    let silent = LoopbackRelay::start(Backend::Silent).await.unwrap();
    let sandbox = Sandbox::new().unwrap();
    let started = Instant::now();

    let run = run_listing(
        &sandbox,
        &["--output", "json", "order", "list"],
        &mostro,
        &[&silent],
    )
    .await;

    // Production budgets: 15 s read plus at most 1 s cleanup, after connecting.
    let defaults = ReadLimits::default();
    assert!(
        started.elapsed() >= defaults.read,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(run.code(), Some(1));
    let outcome = one_document(&run);
    assert_eq!(outcome["error"]["code"], "read_timeout", "{outcome}");
    assert_counts(&outcome["meta"]["snapshot"], 1, 0, 0);
    assert_no_effects(&sandbox, "timeout");
}

#[tokio::test]
async fn order_list_binary_ignores_existing_and_unusable_identity_locations() {
    let mostro = Keys::generate();
    let relay = serve(LocalRelay::new(), &live_book(&mostro)).await;
    let json = ["--output", "json", "order", "list"];

    let existing = Sandbox::new().unwrap();
    let database = existing.mcli_dir().join("mcli.db");
    fs::write(&database, b"synthetic identity store").unwrap();
    let run = run_listing(&existing, &json, &mostro, &[&relay]).await;
    assert_eq!(run.code(), Some(0));
    assert_eq!(
        one_document(&run)["data"]["orders"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(fs::read(&database).unwrap(), b"synthetic identity store");
    let entries: Vec<_> = fs::read_dir(existing.mcli_dir()).unwrap().collect();
    assert_eq!(entries.len(), 1, "only the existing store may be present");

    // MCLI_DIR naming a regular file cannot hold an identity database.
    let unusable = Sandbox::new().unwrap();
    let blocker = unusable.cwd().join("not-a-directory");
    fs::write(&blocker, b"x").unwrap();
    let mut command = unusable.command(json);
    set_node_env(&mut command, &mostro, &[&relay]);
    command.env("MCLI_DIR", &blocker);
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped);
    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert_eq!(
        one_document(&run)["data"]["orders"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(fs::read(&blocker).unwrap(), b"x");
    for dir in [unusable.home(), unusable.mcli_dir()] {
        assert!(is_empty_dir(&dir), "{dir:?} was written");
    }
}

// Human parity (T035): every human invocation shows the JSON business fields.

/// A pending EUR order with explicit sats, fiat tag values and premium.
fn priced_listing(
    mostro: &Keys,
    id: u128,
    created_at: u64,
    side: &str,
    (sats, fiat, premium): (&str, &[&str], &str),
) -> Event {
    EventBuilder::new(Kind::Custom(NOSTR_ORDER_EVENT_KIND), "")
        .tags([
            Tag::custom("d", [Uuid::from_u128(id).to_string()]),
            Tag::custom("k", [side]),
            Tag::custom("s", ["pending"]),
            Tag::custom("f", ["EUR"]),
            Tag::custom("amt", [sats]),
            Tag::custom("fa", fiat.iter().copied()),
            Tag::custom("pm", ["sepa", "cash"]),
            Tag::custom("premium", [premium]),
            Tag::custom("z", ["order"]),
        ])
        .custom_created_at(Timestamp::from(created_at))
        .finalize(mostro)
        .unwrap()
}

/// Trimmed cells of each human table row, keyed by order id.
fn human_rows(table: &str) -> BTreeMap<String, Vec<String>> {
    table
        .lines()
        .filter_map(|line| {
            let cells: Vec<String> = line
                .split(['\u{2502}', '\u{2506}'])
                .map(|cell| cell.trim().to_string())
                .collect();
            Uuid::parse_str(cells.get(2)?).ok()?;
            Some((cells[2].clone(), cells))
        })
        .collect()
}

/// The human cells after id and status that the data model derives from one
/// JSON order: market for zero sats, `?` for a missing range end, UTC minute.
fn human_view_of(order: &Value) -> Vec<String> {
    let text = |field: &str| order[field].as_str().unwrap().to_string();
    let sats = match text("sats_amount") {
        zero if zero == "0" => "market".to_string(),
        sats => sats,
    };
    let fiat = match (
        order["fiat_amount_min"].as_str(),
        order["fiat_amount_max"].as_str(),
    ) {
        (None, None) => text("fiat_amount"),
        (min, max) => format!("{}-{}", min.unwrap_or("?"), max.unwrap_or("?")),
    };
    let created = order["created_at"].as_str().map_or_else(
        || "Invalid date".to_string(),
        |at| at[..16].replace('T', " "),
    );
    vec![
        sats,
        text("fiat_currency"),
        fiat,
        text("payment_method"),
        text("premium_percent"),
        created,
    ]
}

#[tokio::test]
async fn order_list_human_and_json_describe_the_same_book() {
    let mostro = Keys::generate();
    let ago = |hours: u64| Timestamp::now().as_secs() - hours * HOUR;
    let mut book = live_book(&mostro);
    book.extend([
        priced_listing(&mostro, 4, ago(4), "sell", ("21000", &["10", "20"], "-3")),
        priced_listing(&mostro, 5, ago(5), "buy", ("-5", &["50"], "0")),
    ]);
    let relay = serve(LocalRelay::new(), &book).await;
    let stored = stored_ids(&relay).await;
    let verbose_line = format!(
        "verbose: order.list ok; {} events from 1/1 relays\n",
        stored.len()
    );

    let fresh = Sandbox::new().unwrap();
    let existing = Sandbox::new().unwrap();
    let database = existing.mcli_dir().join("mcli.db");
    fs::write(&database, b"synthetic identity store").unwrap();

    for sandbox in [&fresh, &existing] {
        let json = ["--output", "json", "order", "list"];
        let outcome = one_document(&run_listing(sandbox, &json, &mostro, &[&relay]).await);
        assert_eq!(
            row_ids(&outcome),
            [1, 2, 4, 5].map(|id| Uuid::from_u128(id).to_string())
        );

        // Piped stdout throughout: the default never depends on a terminal.
        let human: [(&[&str], &str); 3] = [
            (&["order", "list"], ""),
            (&["order", "list", "--output", "human"], ""),
            (&["--verbose", "order", "list"], &verbose_line),
        ];
        let mut tables = Vec::new();
        for (args, stderr) in human {
            let run = run_listing(sandbox, args, &mostro, &[&relay]).await;
            assert_eq!(run.code(), Some(0), "{args:?}: {}", run.stderr_text());
            assert_eq!(run.stderr_text(), stderr, "{args:?}");
            tables.push(run.stdout_text());
        }
        let mut closed_stdin = sandbox.command(["order", "list"]);
        set_node_env(&mut closed_stdin, &mostro, &[&relay]);
        closed_stdin.stdin(Stdio::piped());
        let run = run_with_watchdog(closed_stdin, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        tables.push(run.stdout_text());

        let table = &tables[0];
        assert!(tables.iter().all(|other| other == table), "{tables:#?}");
        assert!(!table.trim_start().starts_with('{'), "{table}");
        let rows = human_rows(table);
        let orders = outcome["data"]["orders"].as_array().unwrap();
        assert_eq!(rows.len(), orders.len(), "{table}");
        for order in orders {
            let cells = &rows[order["id"].as_str().unwrap()];
            assert!(
                cells[1].contains(order["side"].as_str().unwrap()),
                "{cells:?}"
            );
            assert!(
                cells[3].contains(order["status"].as_str().unwrap()),
                "{cells:?}"
            );
            assert_eq!(cells[4..10], human_view_of(order), "{table}");
        }
    }

    assert_no_effects(&fresh, "fresh identity location");
    assert_eq!(fs::read(&database).unwrap(), b"synthetic identity store");
    assert_eq!(fs::read_dir(existing.mcli_dir()).unwrap().count(), 1);
    for dir in [existing.home(), existing.cwd()] {
        assert!(is_empty_dir(&dir), "{dir:?} was written");
    }
    assert_eq!(stored_ids(&relay).await, stored, "a listing published");
}

// Diagnostics (T032): verbose and `RUST_LOG=trace` must not leak raw SDK,
// relay, URL or argument text on the migrated path.

/// Answers every REQ with CLOSED carrying `reason`.
#[derive(Debug)]
struct RejectSaying(&'static str);

impl QueryPolicy for RejectSaying {
    fn admit_query<'a>(
        &'a self,
        _query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        Box::pin(async { QueryPolicyResult::reject(MachineReadablePrefix::Error, self.0) })
    }
}

const CLOSED_SENTINEL: &str = "relay-says secret-sentinel-7f3a";

/// The relay URL with a sentinel in its query, which the relay ignores.
fn sentinel_url(relay: &LoopbackRelay) -> String {
    format!("{}/?token={SENTINEL}", relay.url)
}

/// Run `order list` verbosely with full logging requested, plus `extra`
/// arguments before the command and `env` overrides.
async fn run_verbose(json: bool, extra: &[&str], env: &[(&str, &str)]) -> (Captured, Sandbox) {
    let sandbox = Sandbox::new().unwrap();
    let mut args = vec!["--verbose"];
    args.extend_from_slice(extra);
    if json {
        args.extend(["--output", "json"]);
    }
    args.extend(["order", "list"]);
    let mut command = sandbox.command(&args);
    command.env("RUST_LOG", "trace");
    for (key, value) in env {
        command.env(key, value);
    }
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped && run.status.is_some(), "{args:?}");
    (run, sandbox)
}

/// No sentinel anywhere, and every stderr line comes from the allow-list.
fn assert_allow_listed(run: &Captured, label: &str) {
    let (stdout, stderr) = (run.stdout_text(), run.stderr_text());
    for (stream, text) in [("stdout", &stdout), ("stderr", &stderr)] {
        assert!(!text.contains(SENTINEL), "{label}: {stream} leaked: {text}");
        assert!(
            !text.contains("127.0.0.1"),
            "{label}: {stream} leaked a URL: {text}"
        );
    }
    let lines: Vec<&str> = stderr.lines().collect();
    assert!(
        lines
            .iter()
            .all(|line| line.starts_with("verbose: order.list ") || line.starts_with("error: ")),
        "{label}: {stderr}"
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("verbose: "))
            .count(),
        1,
        "{label}: {stderr}"
    );
}

#[tokio::test]
async fn order_list_diagnostics_are_allow_listed_in_verbose_mode() {
    let mostro = Keys::generate();
    let mostro_hex = mostro.public_key().to_hex();
    let book = serve(LocalRelay::new(), &live_book(&mostro)).await;
    let closed = LoopbackRelay::start(Backend::Relay(Box::new(
        LocalRelay::builder()
            .query_policy(RejectSaying(CLOSED_SENTINEL))
            .build(),
    )))
    .await
    .unwrap();
    let (book_url, closed_url) = (sentinel_url(&book), sentinel_url(&closed));
    let bad_relays = format!("not-a-url-{SENTINEL}");
    let (key, book_url, closed_url) = (mostro_hex.as_str(), book_url.as_str(), closed_url.as_str());
    type Row<'a> = (&'a str, Vec<&'a str>, Vec<(&'a str, &'a str)>, i32, &'a str);
    let rows: Vec<Row> = vec![
        (
            "success over a URL with a secret query",
            vec![],
            vec![("MOSTRO_PUBKEY", key), ("RELAYS", book_url)],
            0,
            "verbose: order.list ok; 3 events from 1/1 relays",
        ),
        (
            "CLOSED with secret server text",
            vec![],
            vec![("MOSTRO_PUBKEY", key), ("RELAYS", closed_url)],
            1,
            "verbose: order.list relay_read_failed; 0 events from 0/1 relays",
        ),
        (
            "secret environment pubkey",
            vec![],
            vec![("MOSTRO_PUBKEY", SENTINEL), ("RELAYS", book_url)],
            1,
            "verbose: order.list configuration_invalid; no read",
        ),
        (
            "secret CLI pubkey",
            vec!["--mostro-pubkey", SENTINEL],
            vec![("RELAYS", book_url)],
            1,
            "verbose: order.list configuration_invalid; no read",
        ),
        (
            "secret CLI relays",
            vec!["--relays", bad_relays.as_str()],
            vec![("MOSTRO_PUBKEY", key)],
            1,
            "verbose: order.list configuration_invalid; no read",
        ),
    ];
    for (label, extra, env, exit, summary) in rows {
        for json in [true, false] {
            let label = format!("{label} (json: {json})");
            let (run, sandbox) = run_verbose(json, &extra, &env).await;
            assert_eq!(run.code(), Some(exit), "{label}: {}", run.stderr_text());
            assert_allow_listed(&run, &label);
            assert!(
                run.stderr_text().lines().any(|line| line == summary),
                "{label}: {}",
                run.stderr_text()
            );
            if json {
                let stdout = run.stdout_text();
                assert_eq!(stdout.lines().count(), 1, "{label}: {stdout}");
                let outcome: Value = serde_json::from_str(&stdout).unwrap();
                assert_eq!(outcome["status"], if exit == 0 { "ok" } else { "error" });
            } else if exit != 0 {
                assert!(run.stdout.is_empty(), "{label}: {}", run.stdout_text());
            }
            assert_no_effects(&sandbox, &label);
        }
    }
}
