//! Process and exchange tests for `order info` (spec 002).
//!
//! Slice 1 holds only the self-tests of the shared fixtures: the scripted fake
//! daemon on the loopback relay and the identity-store seeding. The command's
//! own scenarios are added with the slices that implement it.

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use mostro_client::msg::{
    exchange, ExchangeFailure, ExchangeLimits, Exchanged, Expect, Outbound, PublicReadConfig,
};
use mostro_client::outcome::{DaemonAcceptance, ErrorCode, Publication};
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, Message, Payload, Transport, WrapOptions,
};
use nostr_sdk::prelude::{
    Client, ClientNotification, Event, Filter, Keys, Kind, LocalRelay, MachineReadablePrefix,
    WritePolicy, WritePolicyResult,
};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{
    always_orders, info_event, malformed_reply, orders_reply, refusal, seed_identity, silent,
    FakeMostro, Outgoing, Received, Respond,
};
use harness::{
    closed_pipe, is_empty_dir, run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox,
    WATCHDOG,
};

const REQUEST_ID: u64 = 8721;

fn order(id: Uuid) -> SmallOrder {
    SmallOrder::new(
        Some(id),
        Some(OrderKind::Sell),
        Some(Status::Active),
        15_000,
        "USD".to_string(),
        None,
        None,
        15,
        "bank transfer".to_string(),
        1,
        None,
        None,
        None,
        Some(1_792_108_800),
        Some(1_792_195_200),
    )
}

/// A loopback relay holding the node's info event, and the fake daemon on it.
async fn daemon(respond: Respond) -> (LoopbackRelay, FakeMostro) {
    let node = Keys::generate();
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&node, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, node, respond).await;
    (relay, fake)
}

/// A requester client with the notification stream armed before it sends.
struct Requester {
    client: Client,
    keys: Keys,
    node: Keys,
}

impl Requester {
    async fn connect(relay: &LoopbackRelay, node: &Keys) -> Self {
        let keys = Keys::generate();
        let client = Client::default();
        client.add_relay(relay.url.as_str()).await.unwrap();
        let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
        assert!(connected.failed.is_empty(), "{:?}", connected.failed);
        Self {
            client,
            keys,
            node: node.clone(),
        }
    }

    /// Subscribe to messages for this key, publish the request, and collect
    /// the decoded replies seen within `wait`.
    async fn ask(&self, ids: Vec<Uuid>, wait: Duration) -> Vec<(Option<u64>, Action)> {
        let mut notifications = self.client.notifications();
        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .pubkey(self.keys.public_key())
            .limit(0);
        self.client.subscribe(filter).await.unwrap();
        let request = Message::new_order(
            None,
            Some(REQUEST_ID),
            None,
            Action::Orders,
            Some(Payload::Ids(ids)),
        );
        let event: Event = wrap_message_with(
            Transport::Nip44Direct,
            &request,
            &self.keys,
            &self.keys,
            self.node.public_key(),
            WrapOptions::default(),
        )
        .await
        .unwrap();
        let sent = event.id;
        self.client.send_event(&event).await.unwrap();

        let mut replies = Vec::new();
        let _ = timeout(wait, async {
            while let Some(notification) = notifications.next().await {
                let ClientNotification::Event { event, .. } = notification else {
                    continue;
                };
                if event.kind != Kind::PrivateDirectMessage || event.id == sent {
                    continue;
                }
                if let Ok(Some(seen)) = unwrap_incoming(&event, &self.keys).await {
                    assert_eq!(seen.sender, self.node.public_key());
                    let inner = seen.message.get_inner_message_kind();
                    replies.push((inner.request_id, inner.action.clone()));
                }
            }
        })
        .await;
        replies
    }
}

#[tokio::test]
async fn the_fake_daemon_records_a_request_and_replies_by_script() {
    let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    let (relay, fake) = daemon(always_orders(vec![order(ids[0])])).await;
    let requester = Requester::connect(&relay, &fake.keys).await;

    let replies = requester.ask(ids.clone(), Duration::from_millis(800)).await;

    assert_eq!(replies, vec![(Some(REQUEST_ID), Action::Orders)]);
    let seen = fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].request_id, Some(REQUEST_ID));
    assert_eq!(seen[0].action, Action::Orders);
    assert_eq!(seen[0].ids, ids);
    assert_eq!(seen[0].author, requester.keys.public_key());
    assert_eq!(seen[0].identity, requester.keys.public_key());
}

#[tokio::test]
async fn the_fake_daemon_scripts_noise_before_the_reply_and_silence() {
    let noisy: Respond = Arc::new(|request| {
        let other = Message::new_order(
            None,
            Some(request.request_id.unwrap() + 1),
            None,
            Action::Orders,
            None,
        );
        vec![
            Outgoing::to_requester(other),
            Outgoing::to_requester(orders_reply(request, Vec::new()))
                .after(Duration::from_millis(50)),
        ]
    });
    let (relay, fake) = daemon(noisy).await;
    let requester = Requester::connect(&relay, &fake.keys).await;
    let replies = requester
        .ask(vec![Uuid::new_v4()], Duration::from_millis(800))
        .await;
    assert_eq!(
        replies,
        vec![
            (Some(REQUEST_ID + 1), Action::Orders),
            (Some(REQUEST_ID), Action::Orders)
        ]
    );

    let (relay, fake) = daemon(silent()).await;
    let requester = Requester::connect(&relay, &fake.keys).await;
    let replies = requester
        .ask(vec![Uuid::new_v4()], Duration::from_millis(400))
        .await;
    assert!(replies.is_empty());
    assert_eq!(fake.received().len(), 1, "the request still arrived");
}

#[tokio::test]
async fn a_seeded_identity_is_a_real_store_with_the_production_derived_key() {
    let sandbox = Sandbox::new().unwrap();
    let keys = seed_identity(&sandbox.mcli_dir()).await;

    let path = sandbox.mcli_dir().join("mcli.db");
    let before = fs::read(&path).unwrap();
    assert!(before.len() > 1_000, "an empty file is not a database");
    assert!(!is_empty_dir(&sandbox.mcli_dir()));
    assert!(is_empty_dir(&sandbox.home()) && is_empty_dir(&sandbox.cwd()));

    // The production reader derives the same key from the file.
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=ro", path.display()))
        .await
        .unwrap();
    let derived = mostro_client::db::User::get_identity_keys(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert_eq!(derived.public_key(), keys.public_key());
    assert_eq!(fs::read(&path).unwrap(), before, "reading changed the file");
}

// --- The exchange (spec 002, slice 3) ---------------------------------------

/// Rejects every write with a machine-readable refusal.
#[derive(Debug)]
struct BlockWrites;

impl WritePolicy for BlockWrites {
    fn admit_event<'a>(
        &'a self,
        _event: &'a Event,
        _addr: &'a std::net::SocketAddr,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = WritePolicyResult> + Send + 'a>> {
        Box::pin(async {
            WritePolicyResult::reject(MachineReadablePrefix::Blocked, "writes are off")
        })
    }
}

/// Counts every kind-14 EVENT message the relay receives from anyone but the
/// node, before duplicate detection, so a request sent twice counts twice.
#[derive(Debug)]
struct CountRequests {
    node: nostr_sdk::prelude::PublicKey,
    count: Arc<AtomicUsize>,
}

impl WritePolicy for CountRequests {
    fn admit_event<'a>(
        &'a self,
        event: &'a Event,
        _addr: &'a std::net::SocketAddr,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = WritePolicyResult> + Send + 'a>> {
        if event.kind == Kind::PrivateDirectMessage && event.pubkey != self.node {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
        Box::pin(async { WritePolicyResult::Accept })
    }
}

/// A relay, a fake daemon on it and the keys of a requester.
struct World {
    relay: LoopbackRelay,
    fake: FakeMostro,
    identity: Keys,
    config: PublicReadConfig,
    /// Request events the relay was sent.
    requests: Arc<AtomicUsize>,
}

fn limits() -> ExchangeLimits {
    ExchangeLimits {
        connect: Duration::from_secs(2),
        probe: Duration::from_millis(400),
        reply: Duration::from_millis(1_500),
        cleanup: Duration::from_millis(500),
        ..ExchangeLimits::default()
    }
}

/// A world whose info event is issued for the node it creates.
async fn world(info: impl FnOnce(&Keys) -> Option<Event>, respond: Respond) -> World {
    world_on(
        |node, requests| {
            LocalRelay::builder()
                .write_policy(CountRequests {
                    node: node.public_key(),
                    count: requests,
                })
                .build()
        },
        info,
        respond,
    )
    .await
}

/// Like [`world`], on a relay that `make` builds from the node's keys and the
/// request counter.
async fn world_on(
    make: impl FnOnce(&Keys, Arc<AtomicUsize>) -> LocalRelay,
    info: impl FnOnce(&Keys) -> Option<Event>,
    respond: Respond,
) -> World {
    // The node keys must exist before the info event, so build them first.
    let node = Keys::generate();
    let requests = Arc::new(AtomicUsize::new(0));
    let relay = make(&node, Arc::clone(&requests));
    if let Some(event) = info(&node) {
        relay.add_event(event).await.unwrap();
    }
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, node.clone(), respond).await;
    let config = PublicReadConfig::new(node.public_key(), &relay.url).unwrap();
    World {
        relay,
        fake,
        identity: Keys::generate(),
        config,
        requests,
    }
}

fn plain_info(node: &Keys) -> Option<Event> {
    Some(info_event(node, Some(2), Some(0), None, Some(10)))
}

fn orders_request(world: &World, ids: Vec<Uuid>) -> Message {
    let _ = &world.fake;
    Message::new_order(
        None,
        Some(REQUEST_ID),
        None,
        Action::Orders,
        Some(Payload::Ids(ids)),
    )
}

fn outbound(world: &World, ids: Vec<Uuid>) -> Outbound<'_> {
    Outbound {
        signer: &world.identity,
        recipient: world.config.mostro_pubkey(),
        message: orders_request(world, ids),
        pow: 0,
        check_protocol: true,
    }
}

fn expect_orders() -> Expect {
    Expect {
        allowed_actions: vec![Action::Orders, Action::CantDo],
    }
}

async fn run(world: &World, ids: Vec<Uuid>) -> Result<Exchanged, ExchangeFailure> {
    exchange(
        &world.config,
        outbound(world, ids),
        &expect_orders(),
        &limits(),
    )
    .await
}

fn failure(result: Result<Exchanged, ExchangeFailure>) -> ExchangeFailure {
    match result {
        Err(failure) => failure,
        Ok(exchanged) => panic!("expected a failure, got {:?}", exchanged.reply()),
    }
}

/// Kind-14 events authored by `author` that the relay holds.
async fn requests_stored(relay: &LoopbackRelay, author: &Keys) -> usize {
    let observer = Client::default();
    observer.add_relay(relay.url.as_str()).await.unwrap();
    observer.connect().await;
    let filter = Filter::new()
        .kind(Kind::PrivateDirectMessage)
        .author(author.public_key());
    let events = observer
        .fetch_events(filter)
        .timeout(Duration::from_secs(2))
        .await
        .unwrap();
    observer.shutdown().await;
    events.len()
}

fn reply_orders(exchanged: &Exchanged) -> Vec<Option<Uuid>> {
    match &exchanged.reply().get_inner_message_kind().payload {
        Some(Payload::Orders(orders)) => orders.iter().map(|order| order.id).collect(),
        other => panic!("unexpected payload {other:?}"),
    }
}

#[tokio::test]
async fn one_signed_request_is_published_and_its_reply_returned() {
    let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    let world = world(plain_info, always_orders(vec![order(ids[0])])).await;

    let exchanged = run(&world, ids.clone()).await.unwrap();

    assert_eq!(reply_orders(&exchanged), vec![Some(ids[0])]);
    assert!(exchanged.warnings().is_empty());
    let seen = world.fake.received();
    assert_eq!(seen.len(), 1, "exactly one request reaches the node");
    assert_eq!(
        world.requests.load(Ordering::SeqCst),
        1,
        "one EVENT was sent"
    );
    assert_eq!(seen[0].author, world.identity.public_key());
    assert_eq!(seen[0].identity, world.identity.public_key());
    assert_eq!(seen[0].request_id, Some(REQUEST_ID));
    assert_eq!(seen[0].ids, ids);
    assert_eq!(requests_stored(&world.relay, &world.identity).await, 1);
}

#[tokio::test]
async fn a_fast_reply_is_never_missed() {
    // The daemon answers the instant it sees the request: the wait must be
    // armed before the request is published.
    let world = world(plain_info, always_orders(Vec::new())).await;
    for round in 0..20 {
        let exchanged = run(&world, vec![Uuid::new_v4()]).await;
        assert!(
            exchanged.is_ok(),
            "round {round}: {:?}",
            exchanged.err().map(|f| f.code())
        );
    }
}

#[tokio::test]
async fn unrelated_traffic_around_the_reply_never_decides_the_result() {
    let id = Uuid::new_v4();
    let outsider = Keys::generate();
    let someone_else = Keys::generate().public_key();
    let noisy: Respond = Arc::new(move |request: &Received| {
        let id_reply = |request_id: Option<u64>| {
            Message::new_order(
                None,
                request_id,
                None,
                Action::Orders,
                Some(Payload::Orders(Vec::new())),
            )
        };
        vec![
            // Another request's reply, an unrelated action with this id, a
            // reply signed by someone other than the node, one addressed to
            // someone else, and a duplicate: none of them is the answer.
            Outgoing::to_requester(id_reply(request.request_id.map(|n| n + 1))),
            Outgoing::to_requester(Message::new_order(
                None,
                request.request_id,
                None,
                Action::Release,
                None,
            )),
            Outgoing::to_requester(id_reply(request.request_id)).signed_by(outsider.clone()),
            Outgoing::to_requester(id_reply(request.request_id)).addressed_to(someone_else),
            Outgoing::to_requester(orders_reply(request, vec![order(id)]))
                .after(Duration::from_millis(50)),
            Outgoing::to_requester(orders_reply(request, vec![order(id)]))
                .after(Duration::from_millis(80)),
        ]
    });
    let world = world(plain_info, noisy).await;

    let exchanged = run(&world, vec![id]).await.unwrap();

    assert_eq!(reply_orders(&exchanged), vec![Some(id)]);
    assert_eq!(world.fake.received().len(), 1);
}

#[tokio::test]
async fn only_traffic_that_is_not_the_answer_ends_in_a_timeout() {
    let outsider = Keys::generate();
    let wrong_only: Respond = Arc::new(move |request: &Received| {
        vec![
            Outgoing::to_requester(orders_reply(request, Vec::new())).signed_by(outsider.clone()),
            Outgoing::to_requester(Message::new_order(
                None,
                request.request_id.map(|n| n + 1),
                None,
                Action::Orders,
                None,
            )),
        ]
    });
    let world = world(plain_info, wrong_only).await;

    let started = std::time::Instant::now();
    let failed = failure(run(&world, vec![Uuid::new_v4()]).await);

    assert_eq!(failed.code(), ErrorCode::ReplyTimeout);
    assert_eq!(
        (failed.publication(), failed.daemon_acceptance()),
        (Publication::Published, DaemonAcceptance::Unknown)
    );
    assert!(started.elapsed() < limits().reply + limits().cleanup + Duration::from_secs(2));
    assert_eq!(world.fake.received().len(), 1, "no retry");
}

#[tokio::test]
async fn a_refusal_and_an_odd_payload_are_returned_for_the_caller_to_judge() {
    let refuse: Respond = Arc::new(|request: &Received| {
        vec![Outgoing::to_requester(refusal(
            request,
            mostro_core::prelude::CantDoReason::TooManyRequests,
        ))]
    });
    let world = world(plain_info, refuse).await;
    let exchanged = run(&world, vec![Uuid::new_v4()]).await.unwrap();
    assert_eq!(
        exchanged.reply().get_inner_message_kind().action,
        Action::CantDo
    );

    let odd: Respond =
        Arc::new(|request: &Received| vec![Outgoing::to_requester(malformed_reply(request))]);
    let world = self::world(plain_info, odd).await;
    let exchanged = run(&world, vec![Uuid::new_v4()]).await.unwrap();
    assert_eq!(
        exchanged.reply().get_inner_message_kind().action,
        Action::Orders
    );
}

#[tokio::test]
async fn silence_is_a_timeout_after_one_publication_with_bounded_cleanup() {
    let world = world(plain_info, silent()).await;
    let open_before = world.relay.open.load(Ordering::SeqCst);

    let started = std::time::Instant::now();
    let failed = failure(run(&world, vec![Uuid::new_v4()]).await);
    let elapsed = started.elapsed();

    assert_eq!(failed.code(), ErrorCode::ReplyTimeout);
    assert_eq!(
        (failed.publication(), failed.daemon_acceptance()),
        (Publication::Published, DaemonAcceptance::Unknown)
    );
    assert!(elapsed >= limits().reply, "{elapsed:?}");
    assert!(
        elapsed < limits().reply + limits().cleanup + Duration::from_secs(2),
        "{elapsed:?}"
    );
    assert_eq!(
        world.fake.received().len(),
        1,
        "the request is not repeated"
    );
    assert_eq!(
        world.requests.load(Ordering::SeqCst),
        1,
        "no second EVENT was sent"
    );
    // Our connection is gone; only the fake daemon's stays open.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(world.relay.open.load(Ordering::SeqCst), open_before);
}

#[tokio::test]
async fn the_node_info_decides_protocol_and_work_before_anything_is_published() {
    // Protocol 1 is refused; an explicit choice skips the check.
    let world = world(
        |node| Some(info_event(node, Some(1), Some(0), None, None)),
        always_orders(Vec::new()),
    )
    .await;
    let failed = failure(run(&world, vec![Uuid::new_v4()]).await);
    assert_eq!(failed.code(), ErrorCode::ProtocolUnsupported);
    assert_eq!(
        (failed.publication(), failed.daemon_acceptance()),
        (Publication::NotAttempted, DaemonAcceptance::Unknown)
    );
    assert!(world.fake.received().is_empty(), "nothing was published");
    let mut skipped = outbound(&world, vec![Uuid::new_v4()]);
    skipped.check_protocol = false;
    assert!(
        exchange(&world.config, skipped, &expect_orders(), &limits())
            .await
            .is_ok()
    );

    // More work expected than configured: fail early; enough work: proceed.
    let world = self::world(
        |node| Some(info_event(node, Some(2), Some(12), None, None)),
        always_orders(Vec::new()),
    )
    .await;
    let failed = failure(run(&world, vec![Uuid::new_v4()]).await);
    assert_eq!(failed.code(), ErrorCode::PowRequired);
    assert_eq!(failed.publication(), Publication::NotAttempted);
    assert!(world.fake.received().is_empty());
    let mut mined = outbound(&world, vec![Uuid::new_v4()]);
    mined.pow = 12;
    assert!(exchange(&world.config, mined, &expect_orders(), &limits())
        .await
        .is_ok());
}

#[tokio::test]
async fn a_missing_info_event_is_not_an_error() {
    let world = world(|_| None, always_orders(Vec::new())).await;
    assert!(run(&world, vec![Uuid::new_v4()]).await.is_ok());
}

#[tokio::test]
async fn work_expected_on_a_first_contact_explains_a_timeout() {
    // Not checked before sending (the identity key may not be a first contact),
    // but a silent node plus this advertisement means the request was dropped.
    let world = world(
        |node| Some(info_event(node, Some(2), Some(0), Some(16), None)),
        silent(),
    )
    .await;
    let failed = failure(run(&world, vec![Uuid::new_v4()]).await);
    assert_eq!(failed.code(), ErrorCode::PowRequired);
    assert_eq!(
        (failed.publication(), failed.daemon_acceptance()),
        (Publication::Published, DaemonAcceptance::Unknown)
    );
    assert_eq!(world.fake.received().len(), 1);
}

#[tokio::test]
async fn a_relay_that_rejects_the_request_leaves_nothing_published() {
    let relay = LocalRelay::builder().write_policy(BlockWrites).build();
    let node = Keys::generate();
    relay
        .add_event(info_event(&node, Some(2), Some(0), None, None))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let config = PublicReadConfig::new(node.public_key(), &relay.url).unwrap();
    let identity = Keys::generate();
    let request = Message::new_order(
        None,
        Some(REQUEST_ID),
        None,
        Action::Orders,
        Some(Payload::Ids(vec![Uuid::new_v4()])),
    );
    let outbound = Outbound {
        signer: &identity,
        recipient: node.public_key(),
        message: request,
        pow: 0,
        check_protocol: true,
    };

    let failed = failure(exchange(&config, outbound, &expect_orders(), &limits()).await);

    assert_eq!(failed.code(), ErrorCode::RequestRejected);
    assert_eq!(
        (failed.publication(), failed.daemon_acceptance()),
        (Publication::NotAttempted, DaemonAcceptance::Unknown)
    );
}

#[tokio::test]
async fn an_unreachable_relay_and_a_closed_subscription_are_told_apart() {
    // Unreachable: nothing was attempted.
    let port = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let node = Keys::generate();
    let config =
        PublicReadConfig::new(node.public_key(), &format!("ws://127.0.0.1:{port}")).unwrap();
    let identity = Keys::generate();
    let request = Message::new_order(
        None,
        Some(REQUEST_ID),
        None,
        Action::Orders,
        Some(Payload::Ids(vec![Uuid::new_v4()])),
    );
    let unreachable = Outbound {
        signer: &identity,
        recipient: node.public_key(),
        message: request,
        pow: 0,
        check_protocol: true,
    };
    let failed = failure(exchange(&config, unreachable, &expect_orders(), &limits()).await);
    assert_eq!(failed.code(), ErrorCode::RelayConnectionFailed);
    assert_eq!(
        (failed.publication(), failed.daemon_acceptance()),
        (Publication::NotAttempted, DaemonAcceptance::Unknown)
    );

    // A relay that answers every REQ with CLOSED: the request went out, the
    // wait can never succeed.
    let (closed, _) = start_fixture_closed().await;
    let config = PublicReadConfig::new(node.public_key(), &closed.url).unwrap();
    let request = Message::new_order(
        None,
        Some(REQUEST_ID),
        None,
        Action::Orders,
        Some(Payload::Ids(vec![Uuid::new_v4()])),
    );
    let outbound = Outbound {
        signer: &identity,
        recipient: node.public_key(),
        message: request,
        pow: 0,
        check_protocol: true,
    };
    let failed = failure(exchange(&config, outbound, &expect_orders(), &limits()).await);
    assert_eq!(failed.code(), ErrorCode::RelayReadFailed);
    assert_eq!(
        (failed.publication(), failed.daemon_acceptance()),
        (Publication::Published, DaemonAcceptance::Unknown)
    );
}

async fn start_fixture_closed() -> (LoopbackRelay, ()) {
    #[derive(Debug)]
    struct RejectQueries;
    impl nostr_sdk::prelude::QueryPolicy for RejectQueries {
        fn admit_query<'a>(
            &'a self,
            _query: &'a mut Filter,
            _addr: &'a std::net::SocketAddr,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = nostr_sdk::prelude::QueryPolicyResult> + Send + 'a,
            >,
        > {
            Box::pin(async {
                nostr_sdk::prelude::QueryPolicyResult::reject(
                    MachineReadablePrefix::Restricted,
                    "queries disabled",
                )
            })
        }
    }
    let relay = LocalRelay::builder().query_policy(RejectQueries).build();
    (
        LoopbackRelay::start(Backend::Relay(Box::new(relay)))
            .await
            .unwrap(),
        (),
    )
}

#[tokio::test]
async fn a_request_without_an_identifier_fails_before_any_connection() {
    let world = world(plain_info, always_orders(Vec::new())).await;
    let mut request = outbound(&world, vec![Uuid::new_v4()]);
    request.message = Message::new_order(
        None,
        None,
        None,
        Action::Orders,
        Some(Payload::Ids(vec![Uuid::new_v4()])),
    );
    let failed = failure(exchange(&world.config, request, &expect_orders(), &limits()).await);
    assert_eq!(failed.code(), ErrorCode::InternalError);
    assert!(world.fake.received().is_empty());
}

// --- The command, run as a child process (spec 002, slice 4) ----------------

/// What the identity store looks like in the child's data directory.
#[derive(Clone, Copy, Debug)]
enum Store {
    /// A real store with one user.
    Valid,
    /// No file at all.
    Absent,
    /// A database whose `users` table is empty.
    EmptyUsers,
    /// A file that is not a database.
    Garbage,
    /// A database without a `users` table.
    NoUsersTable,
    /// A user row without a mnemonic.
    NullMnemonic,
}

/// A world plus a sandbox with an identity store for the child to read.
struct Bin {
    world: World,
    sandbox: Sandbox,
    identity: Keys,
    /// Names and bytes of the data directory before any run.
    dir_before: Vec<(String, Vec<u8>)>,
}

async fn bin(respond: Respond) -> Bin {
    bin_in(world(plain_info, respond).await, Store::Valid).await
}

async fn bin_in(world: World, store: Store) -> Bin {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let identity = match store {
        Store::Valid => seed_identity(&dir).await,
        Store::Absent => Keys::generate(),
        Store::Garbage => {
            fs::write(dir.join("mcli.db"), b"this is not a sqlite database").unwrap();
            Keys::generate()
        }
        Store::EmptyUsers | Store::NoUsersTable | Store::NullMnemonic => {
            let options = SqliteConnectOptions::new()
                .filename(dir.join("mcli.db"))
                .create_if_missing(true);
            let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
            let schema = match store {
                Store::NoUsersTable => "CREATE TABLE other (x INTEGER)",
                _ => {
                    "CREATE TABLE users (i0_pubkey char(64) PRIMARY KEY, mnemonic TEXT, \
                      last_trade_index INTEGER, created_at INTEGER)"
                }
            };
            sqlx::query(schema).execute(&pool).await.unwrap();
            if matches!(store, Store::NullMnemonic) {
                sqlx::query("INSERT INTO users (i0_pubkey, mnemonic) VALUES ('aa', NULL)")
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            pool.close().await;
            Keys::generate()
        }
    };
    let dir_before = directory(&dir);
    Bin {
        world,
        sandbox,
        identity,
        dir_before,
    }
}

fn directory(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap_or_default(),
            )
        })
        .collect();
    files.sort();
    files
}

impl Bin {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = self.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", self.world.config.mostro_pubkey().to_hex())
            .env("RELAYS", &self.world.relay.url);
        command
    }

    async fn run(&self, args: &[&str]) -> Captured {
        let run = run_with_watchdog(self.command(args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    /// The data directory is exactly as it was (no file added, none changed) and
    /// nothing was created elsewhere.
    fn assert_untouched(&self, label: &str) {
        let now = directory(&self.sandbox.mcli_dir());
        let names =
            |files: &[(String, Vec<u8>)]| files.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
        assert_eq!(
            names(&now),
            names(&self.dir_before),
            "{label}: files appeared or vanished"
        );
        assert!(
            now == self.dir_before,
            "{label}: the identity store changed"
        );
        assert!(
            is_empty_dir(&self.sandbox.home()) && is_empty_dir(&self.sandbox.cwd()),
            "{label}"
        );
    }

    /// Run with environment changes applied after the defaults.
    async fn run_env(&self, args: &[&str], set: &[(&str, &str)], unset: &[&str]) -> Captured {
        let mut command = self.command(args);
        for (name, value) in set {
            command.env(name, value);
        }
        for name in unset {
            command.env_remove(name);
        }
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }
}

fn document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert!(
        stdout.ends_with('\n') && stdout.lines().count() == 1,
        "{stdout}"
    );
    serde_json::from_str(&stdout).unwrap()
}

fn text(n: u128) -> String {
    Uuid::from_u128(n).to_string()
}

const ID: [&str; 2] = ["order", "info"];

fn info_args<'a>(output: &[&'a str], ids: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = output.to_vec();
    args.extend(ID);
    for id in ids {
        args.extend(["--order", id.as_str()]);
    }
    args
}

fn reply_with(orders: Vec<SmallOrder>) -> Respond {
    Arc::new(move |request: &Received| {
        vec![Outgoing::to_requester(orders_reply(
            request,
            orders.clone(),
        ))]
    })
}

#[tokio::test]
async fn one_id_returns_the_order_as_one_document_with_one_request_and_no_effects() {
    let bin = bin(reply_with(vec![order(Uuid::from_u128(1))])).await;
    let ids = [text(1)];

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["command"], "order.info");
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["error"], Value::Null);
    assert_eq!(outcome["warnings"], json!([]));
    assert_eq!(outcome["data"]["not_returned"], json!([]));
    assert_eq!(
        outcome["data"]["node_pubkey"],
        bin.world.config.mostro_pubkey().to_hex()
    );
    let record = &outcome["data"]["orders"][0];
    assert_eq!(record["id"], ids[0].as_str());
    assert_eq!(
        (record["side"].as_str(), record["status"].as_str()),
        (Some("sell"), Some("active"))
    );
    assert_eq!(record["sats_amount"], "15000");
    assert_eq!(record["created_at"], "2026-10-16T00:00:00Z");
    assert_eq!(
        outcome["meta"],
        json!({"publication": "published", "daemon_acceptance": "accepted",
               "persistence": "not_applicable", "snapshot": null})
    );
    let seen = bin.world.fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].author, bin.identity.public_key());
    assert_eq!(seen[0].identity, bin.identity.public_key());
    assert_eq!(seen[0].ids, vec![Uuid::from_u128(1)]);
    assert_eq!(bin.world.requests.load(Ordering::SeqCst), 1);
    bin.assert_untouched("success");
}

#[tokio::test]
async fn a_subset_keeps_request_order_drops_unrequested_orders_and_warns_once() {
    let reply = vec![
        order(Uuid::from_u128(3)),
        order(Uuid::from_u128(99)),
        order(Uuid::from_u128(1)),
        order(Uuid::from_u128(3)),
    ];
    let bin = bin(reply_with(reply)).await;
    let ids = [text(1), text(2), text(3)];

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert_eq!(run.code(), Some(0));
    let outcome = document(&run);
    let returned: Vec<&str> = outcome["data"]["orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    assert_eq!(returned, [ids[0].as_str(), ids[2].as_str()]);
    assert_eq!(outcome["data"]["not_returned"], json!([ids[1]]));
    let codes: Vec<&str> = outcome["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["orders_not_returned"]);
    bin.assert_untouched("subset");
}

#[tokio::test]
async fn a_repeated_id_in_any_case_is_requested_once() {
    let bin = bin(reply_with(vec![order(Uuid::from_u128(1))])).await;
    let upper = text(1).to_uppercase();
    let ids = [text(1), upper, text(1)];

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert_eq!(bin.world.fake.received()[0].ids, vec![Uuid::from_u128(1)]);
    assert_eq!(
        document(&run)["data"]["orders"].as_array().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn not_found_is_the_same_success_as_a_reply_that_leaves_the_ids_out() {
    let refuse: Respond = Arc::new(|request: &Received| {
        vec![Outgoing::to_requester(refusal(
            request,
            mostro_core::prelude::CantDoReason::NotFound,
        ))]
    });
    let bin = bin(refuse).await;
    let ids = [text(5), text(4)];

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["data"]["orders"], json!([]));
    assert_eq!(outcome["data"]["not_returned"], json!([ids[0], ids[1]]));
    assert_eq!(outcome["warnings"][0]["code"], "orders_not_returned");
    assert_eq!(outcome["meta"]["daemon_acceptance"], "accepted");
}

#[tokio::test]
async fn noise_around_the_reply_never_decides_the_result() {
    let outsider = Keys::generate();
    let someone_else = Keys::generate().public_key();
    let id = Uuid::from_u128(1);
    let noisy: Respond = Arc::new(move |request: &Received| {
        let empty = |request_id: Option<u64>| {
            Message::new_order(
                None,
                request_id,
                None,
                Action::Orders,
                Some(Payload::Orders(Vec::new())),
            )
        };
        vec![
            Outgoing::to_requester(empty(request.request_id.map(|n| n + 1))),
            Outgoing::to_requester(empty(request.request_id)).signed_by(outsider.clone()),
            Outgoing::to_requester(empty(request.request_id)).addressed_to(someone_else),
            Outgoing::to_requester(orders_reply(request, vec![order(id)]))
                .after(Duration::from_millis(60)),
        ]
    });
    let bin = bin(noisy).await;
    let ids = [text(1)];

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert_eq!(document(&run)["data"]["orders"][0]["id"], ids[0].as_str());
    assert_eq!(
        bin.world.fake.received().len(),
        1,
        "one request reached the node"
    );
    // The relay counts every kind-14 event not signed by the node: the request
    // and the stranger's noise.
    assert_eq!(bin.world.requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_refusal_is_an_error_with_its_reason_in_both_modes() {
    let refuse: Respond = Arc::new(|request: &Received| {
        vec![Outgoing::to_requester(refusal(
            request,
            mostro_core::prelude::CantDoReason::TooManyRequests,
        ))]
    });
    let bin = bin(refuse).await;
    let ids = [text(1)];

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;
    assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["status"], "error");
    assert_eq!(outcome["data"], Value::Null);
    assert_eq!(outcome["error"]["code"], "node_refused");
    assert_eq!(outcome["error"]["reason"], "too_many_requests");
    assert_eq!(
        outcome["meta"],
        json!({"publication": "published", "daemon_acceptance": "rejected",
               "persistence": "not_applicable", "snapshot": null})
    );

    let human = bin.run(&info_args(&[], &ids)).await;
    assert_eq!(human.code(), Some(1));
    assert!(human.stdout.is_empty());
    assert_eq!(
        human.stderr_text(),
        "error: The node refused the request. (reason: too_many_requests)\n"
    );
    bin.assert_untouched("refusal");
}

#[tokio::test]
async fn a_reply_with_an_unusable_payload_is_reply_malformed() {
    let odd: Respond =
        Arc::new(|request: &Received| vec![Outgoing::to_requester(malformed_reply(request))]);
    let bin = bin(odd).await;
    let ids = [text(1)];

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert_eq!(run.code(), Some(1));
    let outcome = document(&run);
    assert_eq!(outcome["error"]["code"], "reply_malformed");
    assert_eq!(outcome["meta"]["daemon_acceptance"], "unknown");
}

#[tokio::test]
async fn a_silent_node_is_a_timeout_after_the_production_budget_and_one_request() {
    let bin = bin(silent()).await;
    let ids = [text(1)];
    let started = std::time::Instant::now();

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert!(
        started.elapsed() >= ExchangeLimits::default().reply,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["error"]["code"], "reply_timeout");
    assert_eq!(outcome["meta"]["publication"], "published");
    assert_eq!(outcome["meta"]["daemon_acceptance"], "unknown");
    assert_eq!(bin.world.fake.received().len(), 1);
    assert_eq!(bin.world.requests.load(Ordering::SeqCst), 1);
    bin.assert_untouched("timeout");
}

#[tokio::test]
async fn human_and_json_describe_the_same_orders() {
    let id = Uuid::from_u128(1);
    let mut priced = order(id);
    priced.min_amount = Some(10);
    priced.max_amount = Some(20);
    priced.premium = -3;
    priced.payment_method = "bank\u{1b}[31m transfer".to_string();
    let bin = bin(reply_with(vec![priced])).await;
    let ids = [text(1), text(2)];

    let json_run = bin.run(&info_args(&["--output", "json"], &ids)).await;
    let human_run = bin.run(&info_args(&[], &ids)).await;
    let explicit = bin.run(&info_args(&["--output", "human"], &ids)).await;

    assert_eq!(
        (json_run.code(), human_run.code(), explicit.code()),
        (Some(0), Some(0), Some(0))
    );
    assert!(human_run.stderr.is_empty() && explicit.stderr.is_empty());
    assert_eq!(human_run.stdout_text(), explicit.stdout_text());
    let human = human_run.stdout_text();
    let record = &document(&json_run)["data"]["orders"][0];
    for needle in [
        record["id"].as_str().unwrap(),
        "sell",
        "active",
        "15000 sats",
        "USD",
        "Fiat Range: 10-20",
        "Premium: -3%",
        "bank\\u{1b}[31m transfer",
        "2026-10-16 00:00:00 UTC",
        "2026-10-17 00:00:00 UTC",
        ids[1].as_str(),
        "Not returned",
    ] {
        assert!(human.contains(needle), "missing {needle:?} in:\n{human}");
    }
    assert!(
        !human.contains('\u{1b}'),
        "a raw control character reached the terminal"
    );
    assert_eq!(
        bin.world.requests.load(Ordering::SeqCst),
        3,
        "one request per run"
    );
    bin.assert_untouched("parity");
}

#[tokio::test]
async fn the_output_flag_works_at_every_position() {
    let bin = bin(reply_with(vec![order(Uuid::from_u128(1))])).await;
    let id = text(1);
    let spellings: [Vec<&str>; 6] = [
        vec!["--output", "json", "order", "info", "--order", &id],
        vec!["--output=json", "order", "info", "--order", &id],
        vec!["order", "--output", "json", "info", "--order", &id],
        vec!["order", "--output=json", "info", "--order", &id],
        vec!["order", "info", "--order", &id, "--output", "json"],
        vec!["order", "info", "--output=json", "--order", &id],
    ];
    let mut documents = Vec::new();
    for args in &spellings {
        let run = bin.run(args).await;
        assert_eq!(run.code(), Some(0), "{args:?}: {}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{args:?}");
        documents.push(document(&run));
    }
    for other in &documents[1..] {
        assert_eq!(other["data"], documents[0]["data"]);
        assert_eq!(other["meta"], documents[0]["meta"]);
    }
    assert_eq!(bin.world.requests.load(Ordering::SeqCst), 6);
}

#[tokio::test]
async fn many_ids_go_out_in_one_request_without_a_cap_in_the_cli() {
    let bin = bin(reply_with(vec![order(Uuid::from_u128(7))])).await;
    let ids: Vec<String> = (1..=300).map(text).collect();

    let run = bin.run(&info_args(&["--output", "json"], &ids)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    let seen = bin.world.fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].ids.len(), 300);
    assert_eq!(bin.world.requests.load(Ordering::SeqCst), 1);
    let outcome = document(&run);
    assert_eq!(outcome["data"]["orders"].as_array().unwrap().len(), 1);
    assert_eq!(
        outcome["data"]["not_returned"].as_array().unwrap().len(),
        299
    );
}

#[tokio::test]
async fn verbose_adds_one_allow_listed_line_and_never_the_mnemonic_or_the_ids() {
    let bin = bin(reply_with(vec![order(Uuid::from_u128(1))])).await;
    let ids = [text(1)];
    let mut args = vec!["--verbose"];
    args.extend(info_args(&["--output", "json"], &ids));

    let run = bin.run(&args).await;

    assert_eq!(run.code(), Some(0));
    assert_eq!(
        run.stderr_text(),
        "verbose: order.info ok; 1 returned, 0 not returned\n"
    );
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}?mode=ro",
        bin.sandbox.mcli_dir().join("mcli.db").display()
    ))
    .await
    .unwrap();
    let mnemonic: String = sqlx::query_scalar("SELECT mnemonic FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    for stream in [run.stdout_text(), run.stderr_text()] {
        assert!(!stream.contains(&mnemonic));
        for word in mnemonic.split(' ') {
            assert!(
                !stream.contains(&format!(" {word} ")),
                "a mnemonic word reached a stream"
            );
        }
        assert!(
            !stream.contains("127.0.0.1"),
            "a relay address reached a stream"
        );
    }
    bin.assert_untouched("verbose");
}

#[tokio::test]
async fn an_unwritable_stdout_exits_1_with_one_fixed_line_after_one_request() {
    let bin = bin(reply_with(vec![order(Uuid::from_u128(1))])).await;
    let ids = [text(1)];
    for (output, line) in [
        (
            &["--output", "json"][..],
            "error: the JSON outcome could not be written\n",
        ),
        (
            &[][..],
            "error: the order information could not be written\n",
        ),
    ] {
        let mut command = bin.command(&info_args(output, &ids));
        command.stdout(closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1), "{output:?}: {}", run.stderr_text());
        assert_eq!(run.stderr_text(), line, "{output:?}");
    }
    assert_eq!(
        bin.world.requests.load(Ordering::SeqCst),
        2,
        "one request per run"
    );
}

/// One failure of the matrix: where it comes from and what both modes show.
struct Failing {
    label: &'static str,
    store: Store,
    /// Environment added to the child, then variables removed.
    set: &'static [(&'static str, &'static str)],
    unset: &'static [&'static str],
    /// Extra global arguments placed before the command.
    flags: &'static [&'static str],
    code: &'static str,
    exit: i32,
    publication: &'static str,
    acceptance: &'static str,
}

const SENTINEL: &str = "SENTINEL-zq81";

impl Failing {
    const fn new(label: &'static str, store: Store, code: &'static str) -> Self {
        Self {
            label,
            store,
            set: &[],
            unset: &[],
            flags: &[],
            code,
            exit: 1,
            publication: "not_attempted",
            acceptance: "unknown",
        }
    }
}

/// Run `case` in both modes plus `--verbose`, against a world made by `make`,
/// and return the world's request count seen from the relay.
async fn check_failure(case: &Failing, bin: &Bin) {
    let ids = [text(1)];
    let label = case.label;
    for output in [&["--output", "json"][..], &[][..], &["--verbose"][..]] {
        let mut flags: Vec<&str> = case.flags.to_vec();
        flags.extend(output);
        let run = bin
            .run_env(&info_args(&flags, &ids), case.set, case.unset)
            .await;
        assert_eq!(
            run.code(),
            Some(case.exit),
            "{label} {output:?}: {}",
            run.stderr_text()
        );
        let (stdout, stderr) = (run.stdout_text(), run.stderr_text());
        for stream in [&stdout, &stderr] {
            assert!(!stream.contains(SENTINEL), "{label}: a sentinel leaked");
            assert!(
                !stream.contains("127.0.0.1"),
                "{label}: a relay address leaked"
            );
        }
        if output == ["--output", "json"] {
            let outcome = document(&run);
            assert_eq!(outcome["command"], "order.info", "{label}");
            assert_eq!(outcome["status"], "error", "{label}");
            assert_eq!(outcome["data"], Value::Null, "{label}");
            assert_eq!(outcome["error"]["code"], case.code, "{label}");
            assert_eq!(outcome["meta"]["publication"], case.publication, "{label}");
            assert_eq!(
                outcome["meta"]["daemon_acceptance"], case.acceptance,
                "{label}"
            );
            assert_eq!(outcome["meta"]["persistence"], "not_applicable", "{label}");
            assert!(stderr.is_empty(), "{label}: {stderr}");
        } else {
            assert!(stdout.is_empty(), "{label} {output:?}: {stdout}");
            assert!(stderr.starts_with("error: "), "{label}: {stderr}");
            assert_eq!(
                stderr.lines().next().unwrap().matches("error:").count(),
                1,
                "{label}"
            );
        }
    }
    bin.assert_untouched(label);
}

#[tokio::test]
async fn failures_before_any_request_are_stable_and_leave_no_trace() {
    let cases = [
        Failing::new("no store", Store::Absent, "identity_missing"),
        Failing::new("no user", Store::EmptyUsers, "identity_missing"),
        Failing::new("not a database", Store::Garbage, "identity_unreadable"),
        Failing::new("no users table", Store::NoUsersTable, "identity_unreadable"),
        Failing::new("null mnemonic", Store::NullMnemonic, "identity_unreadable"),
        Failing {
            unset: &["MOSTRO_PUBKEY"],
            ..Failing::new("no node key", Store::Valid, "configuration_missing")
        },
        Failing {
            unset: &["RELAYS"],
            ..Failing::new("no relays", Store::Valid, "configuration_missing")
        },
        Failing {
            set: &[("MOSTRO_PUBKEY", SENTINEL)],
            ..Failing::new("bad node key", Store::Valid, "configuration_invalid")
        },
        Failing {
            set: &[("RELAYS", "http://SENTINEL-zq81.invalid")],
            ..Failing::new("bad relay", Store::Valid, "configuration_invalid")
        },
        Failing {
            flags: &["--pow", SENTINEL],
            ..Failing::new("bad pow", Store::Valid, "configuration_invalid")
        },
        Failing {
            flags: &["--transport", SENTINEL],
            ..Failing::new("bad transport", Store::Valid, "configuration_invalid")
        },
    ];
    for case in &cases {
        let bin = bin_in(world(plain_info, silent()).await, case.store).await;
        check_failure(case, &bin).await;
        assert!(bin.world.fake.received().is_empty(), "{}", case.label);
        assert_eq!(
            bin.world.requests.load(Ordering::SeqCst),
            0,
            "{}",
            case.label
        );
    }
}

#[tokio::test]
async fn a_node_that_cannot_be_served_fails_before_publishing() {
    let unsupported = Failing::new("protocol 1", Store::Valid, "protocol_unsupported");
    let bin = bin_in(
        world(
            |node| Some(info_event(node, Some(1), Some(0), None, None)),
            silent(),
        )
        .await,
        Store::Valid,
    )
    .await;
    check_failure(&unsupported, &bin).await;
    assert_eq!(bin.world.requests.load(Ordering::SeqCst), 0);

    let work = Failing::new("more work expected", Store::Valid, "pow_required");
    let bin = bin_in(
        world(
            |node| Some(info_event(node, Some(2), Some(9), None, None)),
            silent(),
        )
        .await,
        Store::Valid,
    )
    .await;
    check_failure(&work, &bin).await;
    assert_eq!(bin.world.requests.load(Ordering::SeqCst), 0);

    // An explicit transport skips the protocol check, and enough work passes.
    let bin = bin_in(
        world(
            |node| Some(info_event(node, Some(1), Some(9), None, None)),
            reply_with(vec![order(Uuid::from_u128(1))]),
        )
        .await,
        Store::Valid,
    )
    .await;
    let ids = [text(1)];
    let mut args = vec!["--transport", "nip44", "--pow", "9", "--output", "json"];
    args.extend(&info_args(&[], &ids));
    let run = bin.run(&args).await;
    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
}

#[tokio::test]
async fn a_relay_that_rejects_the_request_is_request_rejected_with_nothing_published() {
    let bin = bin_in(
        world_on(
            |_, _| LocalRelay::builder().write_policy(BlockWrites).build(),
            plain_info,
            silent(),
        )
        .await,
        Store::Valid,
    )
    .await;
    let case = Failing::new("rejected", Store::Valid, "request_rejected");
    check_failure(&case, &bin).await;
    assert!(bin.world.fake.received().is_empty());
}

#[tokio::test]
async fn relay_failures_are_told_apart_and_cost_one_attempt() {
    // Nothing listens on the configured relay.
    let bin = bin(silent()).await;
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("ws://{}", free.local_addr().unwrap());
    drop(free);
    let case = Failing {
        set: &[],
        ..Failing::new("unreachable", Store::Valid, "relay_connection_failed")
    };
    let ids = [text(1)];
    let run = bin
        .run_env(
            &info_args(&["--output", "json"], &ids),
            &[("RELAYS", dead.as_str())],
            &[],
        )
        .await;
    assert_eq!(run.code(), Some(case.exit), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["error"]["code"], case.code);
    assert_eq!(outcome["meta"]["publication"], "not_attempted");
    assert!(!run.stderr_text().contains("127.0.0.1"));
    assert!(bin.world.fake.received().is_empty());
    bin.assert_untouched("unreachable");
}

#[tokio::test]
async fn argument_errors_name_the_command_only_when_it_resolved() {
    let bin = bin(silent()).await;
    let cases: [(&[&str], Value); 5] = [
        (&["order", "info"], json!("order.info")),
        (
            &["order", "info", "--order", "not-a-uuid"],
            json!("order.info"),
        ),
        (&["order", "info", "--order"], json!("order.info")),
        (
            &["order", "info", "--order", &text(1), "--bogus"],
            json!("order.info"),
        ),
        (&["ordersinfo", "--order", &text(1)], Value::Null),
    ];
    for (args, command) in cases {
        let mut all = vec!["--output", "json"];
        all.extend(args);
        let run = bin.run(&all).await;
        assert_eq!(run.code(), Some(2), "{args:?}: {}", run.stderr_text());
        let outcome = document(&run);
        assert_eq!(outcome["command"], command, "{args:?}");
        assert_eq!(outcome["error"]["code"], "invalid_arguments", "{args:?}");
        assert_eq!(outcome["meta"]["publication"], "not_attempted", "{args:?}");
        assert!(!run.stdout_text().contains("not-a-uuid"), "{args:?}");
    }
    assert!(bin.world.fake.received().is_empty());
    bin.assert_untouched("arguments");
}

#[tokio::test]
async fn an_unwritable_stderr_changes_neither_the_exit_nor_the_document() {
    let bin = bin(silent()).await;
    let ids = [text(1)];
    let mut command = bin.command(&info_args(&["--output", "json", "--pow", "x"], &ids));
    command.stderr(closed_pipe());
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert_eq!(run.code(), Some(1));
    assert_eq!(document(&run)["error"]["code"], "configuration_invalid");
}
