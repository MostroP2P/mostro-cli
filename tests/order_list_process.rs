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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, io, process};

use futures::future::join_all;
use futures::{Stream, StreamExt};
use hyper::body::Incoming;
use hyper::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::{self, OnUpgrade};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use nostr_sdk::prelude::{
    Client, Event, EventBuilder, EventId, Filter, FinalizeEvent, Keys, Kind, LocalRelay,
    MachineReadablePrefix, QueryPolicy, QueryPolicyResult, RelayMessage, RelayNotification,
    RelayStatus, SubscriptionId,
};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Notify;
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

/// Run `args` with node configuration pointing at `endpoint`, so any network
/// or storage effect of a rejected invocation is observable.
async fn run_configured(args: &[&str], endpoint: &LoopbackRelay) -> (Captured, Sandbox) {
    let sandbox = Sandbox::new().unwrap();
    let mut command = sandbox.command(args);
    set_node_env(&mut command, &Keys::generate(), &[endpoint]);
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
    let rows: [&[&str]; 5] = [
        &["listorders", &unknown_flag],
        &[SENTINEL],
        &["--output", SENTINEL, "listorders"],
        &["--output", "json", "--output", "json", "listorders"],
        &["--output", "human", "listorders", &unknown_flag],
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

#[tokio::test]
async fn json_intent_gets_one_error_envelope_before_any_effect() {
    let endpoint = LoopbackRelay::start(Backend::Relay(Box::default()))
        .await
        .unwrap();
    let unknown_flag = format!("--{SENTINEL}");
    let rows: [(&[&str], Value, &str); 6] = [
        (
            &["--output", "json", "listorders"],
            json!("order.list"),
            "output_not_supported",
        ),
        (
            &["listorders", "--output=json"],
            json!("order.list"),
            "output_not_supported",
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

#[tokio::test]
async fn help_and_version_stay_text_on_stdout() {
    let endpoint = LoopbackRelay::start(Backend::Silent).await.unwrap();
    let rows: [&[&str]; 5] = [
        &["--help"],
        &["--output", "json", "--help"],
        &["listorders", "--output", "json", "--help"],
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
