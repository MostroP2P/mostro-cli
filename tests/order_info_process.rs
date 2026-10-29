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
use harness::{is_empty_dir, Backend, LoopbackRelay, Sandbox};

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
    // The node keys must exist before the info event, so build them first.
    let node = Keys::generate();
    let requests = Arc::new(AtomicUsize::new(0));
    let relay = LocalRelay::builder()
        .write_policy(CountRequests {
            node: node.public_key(),
            count: Arc::clone(&requests),
        })
        .build();
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
