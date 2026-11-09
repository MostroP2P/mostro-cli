//! Tests for `order watch` (spec 020): the live subscription (units 2 and 3),
//! the operation (unit 4), the binary in human mode (unit 5) and in JSON
//! (unit 6).
//!
//! The node's mail and the chat sit on in-process loopback relays, signed by
//! generated keys. Nothing else is contacted.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mostro_client::msg::watch::{Batch, Seen, Watch, WatchFailure, WatchLimits};
use mostro_client::msg::{PublicReadConfig, ReadLimits};
use mostro_client::outcome::watch::Cursor;
use mostro_client::outcome::ErrorCode;
use mostro_core::prelude::{derive_chat_keys, wrap_chat_message};
use nostr_sdk::prelude::{
    nip44, Event, EventBuilder, EventId, Filter, FinalizeEvent, Keys, Kind, LocalRelay,
    MachineReadablePrefix, PublicKey, QueryPolicy, QueryPolicyResult, Tag, Timestamp,
};
use tokio::sync::Notify;
use tokio::time::Instant;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{mail_text, node_event};
use harness::{Backend, LoopbackRelay};

const ORDER: Uuid = Uuid::from_u128(0x0a7c4);

fn limits() -> WatchLimits {
    WatchLimits {
        read: ReadLimits {
            connect: Duration::from_secs(2),
            read: Duration::from_millis(1_500),
            cleanup: Duration::from_millis(500),
            ..ReadLimits::default()
        },
        retry: Duration::from_secs(1),
        rate_grace: Duration::from_secs(1),
    }
}

fn now() -> u64 {
    Timestamp::now().as_secs()
}

/// A relay URL nothing listens on.
fn dead_relay() -> String {
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", free.local_addr().unwrap());
    drop(free);
    url
}

fn config(node: &Keys, urls: &[&str]) -> PublicReadConfig {
    PublicReadConfig::new(node.public_key(), &urls.join(",")).unwrap()
}

/// A message of `author` to `to` about the order, as the node sends it.
async fn mail(author: &Keys, to: &Keys, at: u64, text: &str) -> Event {
    let message = mail_text(ORDER, text);
    node_event(author, to.public_key(), &message, Timestamp::from(at)).await
}

/// A loopback relay serving `local`, holding `events`.
async fn serve(local: &LocalRelay, events: &[&Event]) -> LoopbackRelay {
    for event in events {
        local.add_event((*event).clone()).await.unwrap();
    }
    LoopbackRelay::start(Backend::Relay(Box::new(local.clone())))
        .await
        .unwrap()
}

/// A loopback relay serving `local` that the test can drop, holding `events`.
async fn kickable(local: &LocalRelay, events: &[&Event]) -> LoopbackRelay {
    for event in events {
        local.add_event((*event).clone()).await.unwrap();
    }
    let kick = Arc::new(Notify::new());
    LoopbackRelay::start(Backend::Kickable(Box::new(local.clone()), kick))
        .await
        .unwrap()
}

/// Drop every session of `relay` and wait until it is gone.
async fn drop_sessions(relay: &LoopbackRelay) {
    relay.kick();
    assert!(
        relay.wait_idle(OBSERVE).await,
        "the relay dropped the watch"
    );
}

/// Answers every REQ as if it had no filter: everything the relay holds,
/// under the caller's subscription id.
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

/// Refuses every REQ.
#[derive(Debug)]
struct Refuse;

impl QueryPolicy for Refuse {
    fn admit_query<'a>(
        &'a self,
        _query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        let refusal = QueryPolicyResult::reject(MachineReadablePrefix::Restricted, "not served");
        Box::pin(async { refusal })
    }
}

/// Refuses the second REQ it gets with `prefix` and serves every other one;
/// `reqs` counts them all.
#[derive(Debug)]
struct RefuseSecond {
    reqs: Arc<AtomicUsize>,
    prefix: MachineReadablePrefix,
}

impl RefuseSecond {
    fn with(prefix: MachineReadablePrefix) -> Self {
        let reqs = Arc::default();
        Self { reqs, prefix }
    }
}

impl QueryPolicy for RefuseSecond {
    fn admit_query<'a>(
        &'a self,
        _query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        let result = match self.reqs.fetch_add(1, Ordering::SeqCst) {
            1 => QueryPolicyResult::reject(self.prefix.clone(), "not now"),
            _ => QueryPolicyResult::Accept,
        };
        Box::pin(async { result })
    }
}

/// Serves the first REQ it gets and leaves every later one unanswered.
#[derive(Debug, Default)]
struct StallLater(AtomicUsize);

impl QueryPolicy for StallLater {
    fn admit_query<'a>(
        &'a self,
        _query: &'a mut Filter,
        _addr: &'a SocketAddr,
    ) -> Pin<Box<dyn Future<Output = QueryPolicyResult> + Send + 'a>> {
        let first = self.0.fetch_add(1, Ordering::SeqCst) == 0;
        Box::pin(async move {
            if !first {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            QueryPolicyResult::Accept
        })
    }
}

/// A watch that opened.
async fn watching(
    config: &PublicReadConfig,
    trade: &Keys,
    peer: Option<&PublicKey>,
    since: u64,
) -> Watch {
    let start = Cursor::start(since);
    Watch::open(config, (Some(1), trade), peer, &start, &limits())
        .await
        .unwrap()
}

fn ids(batch: &Batch) -> Vec<EventId> {
    batch.items.iter().map(Seen::id).collect()
}

fn texts(batch: &Batch) -> Vec<&str> {
    batch
        .items
        .iter()
        .map(|item| match item {
            Seen::Chat { line, .. } => line.text.as_str(),
            Seen::Mail(_) => "(mail)",
        })
        .collect()
}

fn failed(opened: Result<Watch, WatchFailure>) -> ErrorCode {
    match opened {
        Ok(_) => panic!("the watch opened"),
        Err(failure) => failure.code(),
    }
}

/// The two sides of the order's conversation.
struct Chat {
    trade: Keys,
    them: Keys,
    conv: Keys,
    sign: Keys,
}

impl Chat {
    fn new() -> Self {
        let (trade, them) = (Keys::generate(), Keys::generate());
        let (conv, sign) = derive_chat_keys(&trade, &them.public_key()).unwrap();
        Self {
            trade,
            them,
            conv,
            sign,
        }
    }

    /// What `writer` wrote at `at`, before it is wrapped.
    fn note(writer: &Keys, text: &str, at: u64) -> Event {
        EventBuilder::new(Kind::TextNote, text)
            .custom_created_at(Timestamp::from(at))
            .finalize(writer)
            .unwrap()
    }

    /// `inner` in an envelope of the conversation dated `at`.
    fn envelope(&self, inner: &Event, at: u64) -> Event {
        let content = nip44::encrypt(
            self.conv.secret_key(),
            &self.conv.public_key(),
            inner.as_json(),
            nip44::Version::V2,
        )
        .unwrap();
        EventBuilder::new(Kind::PrivateDirectMessage, content)
            .tags([Tag::public_key(self.conv.public_key())])
            .custom_created_at(Timestamp::from(at))
            .finalize(&self.sign)
            .unwrap()
    }
}

mod live {
    use super::*;

    #[tokio::test]
    async fn the_window_is_replayed_then_new_mail_arrives_live_once() {
        let (node, trade, other, impostor) = (
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
        );
        let base = now();
        let since = base - 1_800;
        let first = mail(&node, &trade, base - 600, "one").await;
        let second = mail(&node, &trade, base - 300, "two").await;
        let noise = [
            mail(&node, &trade, since - 60, "too old").await,
            mail(&node, &other, base - 200, "someone else's").await,
            mail(&impostor, &trade, base - 100, "an impostor").await,
        ];
        let stored: Vec<&Event> = noise.iter().chain([&first, &second]).collect();
        // The second relay sends everything it holds under our ids, the
        // impostor's event included.
        let (plain, loose) = (
            LocalRelay::new(),
            LocalRelay::builder().query_policy(IgnoreFilter).build(),
        );
        let (a, b) = (serve(&plain, &stored).await, serve(&loose, &stored).await);
        let config = config(&node, &[&a.url, &b.url]);

        let mut watch = watching(&config, &trade, None, since).await;
        let replayed = watch.next(Some(Instant::now())).await.unwrap();
        assert_eq!(ids(&replayed), [first.id, second.id], "the window, once");
        assert!(!replayed.truncated);

        let third = mail(&node, &trade, now(), "three").await;
        let late_noise = [
            mail(&node, &other, now(), "someone else's").await,
            mail(&impostor, &trade, now(), "an impostor").await,
        ];
        for relay in [&plain, &loose] {
            for event in late_noise.iter().chain([&third]) {
                relay.add_event(event.clone()).await.unwrap();
            }
        }
        let arrived = watch.next(Some(Instant::now() + OBSERVE)).await.unwrap();
        assert_eq!(ids(&arrived), [third.id], "live, from either relay");
        let quiet = watch.next(Some(Instant::now() + QUIET)).await.unwrap();
        assert!(quiet.items.is_empty(), "a second copy is not new");
        assert_eq!(watch.observed().len(), 3, "the noise is never counted");

        assert!(watch.close().await.is_empty());
        assert!(a.wait_idle(OBSERVE).await && b.wait_idle(OBSERVE).await);
    }

    #[tokio::test]
    async fn the_chat_is_replayed_then_live_when_the_counterparty_is_known() {
        let (node, chat, stranger) = (Keys::generate(), Chat::new(), Keys::generate());
        let base = now();
        let lock = mail(&node, &chat.trade, base - 900, "locked").await;
        let theirs = Chat::note(&chat.them, "paid", base - 500);
        let ours = Chat::note(&chat.trade, "ok", base - 400);
        // Written before the window, in an envelope dated inside it (60 s skew).
        let early = Chat::note(&chat.them, "early", base - 1_810);
        let envelopes = [
            chat.envelope(&early, base - 1_780),
            chat.envelope(&theirs, base - 500),
            chat.envelope(&theirs, base - 450),
            chat.envelope(&ours, base - 400),
            chat.envelope(&Chat::note(&stranger, "forged", base - 300), base - 300),
        ];
        let stored: Vec<&Event> = envelopes.iter().chain([&lock]).collect();
        let (plain, loose) = (
            LocalRelay::new(),
            LocalRelay::builder().query_policy(IgnoreFilter).build(),
        );
        let (a, b) = (serve(&plain, &stored).await, serve(&loose, &stored).await);
        let config = config(&node, &[&a.url, &b.url]);

        let peer = chat.them.public_key();
        let mut watch = watching(&config, &chat.trade, Some(&peer), base - 1_800).await;
        let replayed = watch.next(Some(Instant::now())).await.unwrap();
        let once_each = ["early", "(mail)", "paid", "ok"];
        assert_eq!(texts(&replayed), once_each, "the window, by outer time");
        assert_eq!(ids(&replayed), [early.id, lock.id, theirs.id, ours.id]);

        let line = wrap_chat_message(&chat.them, &chat.conv, &chat.sign, "on my way")
            .await
            .unwrap();
        plain.add_event(line).await.unwrap();
        let arrived = watch.next(Some(Instant::now() + OBSERVE)).await.unwrap();
        assert_eq!(texts(&arrived), ["on my way"]);
        let quiet = watch.next(Some(Instant::now() + QUIET)).await.unwrap();
        assert!(quiet.items.is_empty());

        assert!(watch.close().await.is_empty());
        assert!(a.wait_idle(OBSERVE).await && b.wait_idle(OBSERVE).await);
    }

    #[tokio::test]
    async fn fifty_stored_events_that_do_not_open_still_set_truncated() {
        let (node, chat, stranger) = (Keys::generate(), Chat::new(), Keys::generate());
        let base = now();
        let paid = chat.envelope(&Chat::note(&chat.them, "paid", base - 900), base - 900);
        let forged: Vec<Event> = (0..50)
            .map(|n| {
                chat.envelope(
                    &Chat::note(&stranger, "forged", base - 800 + n),
                    base - 800 + n,
                )
            })
            .collect();
        let relay = serve(
            &LocalRelay::new(),
            &forged.iter().chain([&paid]).collect::<Vec<_>>(),
        )
        .await;
        let (config, peer) = (config(&node, &[&relay.url]), chat.them.public_key());
        let mut watch = watching(&config, &chat.trade, Some(&peer), base - 1_800).await;
        let replayed = watch.next(Some(Instant::now())).await.unwrap();
        assert!(
            replayed.items.is_empty() && replayed.truncated,
            "\"paid\" is hidden"
        );
        watch.close().await;
    }

    #[tokio::test]
    async fn opening_failures_keep_the_read_codes() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let start = Cursor::start(now() - 60);
        let open = |urls: Vec<String>| {
            let config = config(&node, &urls.iter().map(String::as_str).collect::<Vec<_>>());
            let trade = trade.clone();
            let start = start.clone();
            async move { Watch::open(&config, (None, &trade), None, &start, &limits()).await }
        };

        let unreachable = open(vec![dead_relay()]).await;
        assert_eq!(failed(unreachable), ErrorCode::RelayConnectionFailed);

        let silent = LoopbackRelay::start(Backend::Silent).await.unwrap();
        let started = Instant::now();
        let unanswered = open(vec![silent.url.clone()]).await;
        assert_eq!(failed(unanswered), ErrorCode::ReadTimeout);
        let budget = limits().read.read + limits().read.cleanup;
        assert!(started.elapsed() < budget + Duration::from_secs(2));

        let refusing = LocalRelay::builder().query_policy(Refuse).build();
        let refusing = serve(&refusing, &[]).await;
        let fine = serve(&LocalRelay::new(), &[]).await;
        let refused = open(vec![fine.url.clone(), refusing.url.clone()]).await;
        assert_eq!(failed(refused), ErrorCode::RelayReadFailed);
        assert!(fine.wait_idle(OBSERVE).await, "and it cleaned up");
    }

    #[tokio::test]
    async fn close_is_bounded_and_leaves_no_connection() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let relay = serve(&LocalRelay::new(), &[]).await;
        let config = config(&node, &[&relay.url]);
        let watch = watching(&config, &trade, None, now() - 60).await;

        let started = Instant::now();
        let warnings = watch.close().await;

        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(started.elapsed() < limits().read.cleanup + Duration::from_millis(500));
        assert!(relay.wait_idle(OBSERVE).await);
        assert_eq!(relay.connections(), 1);
    }
}

mod reconnect {
    use super::*;

    #[tokio::test]
    async fn a_dropped_relay_reconnects_and_misses_nothing() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let base = now();
        let first = mail(&node, &trade, base - 600, "one").await;
        let local = LocalRelay::new();
        let relay = kickable(&local, &[&first]).await;
        let config = config(&node, &[&relay.url]);
        let mut watch = watching(&config, &trade, None, base - 1_800).await;
        assert_eq!(
            ids(&watch.next(Some(Instant::now())).await.unwrap()),
            [first.id]
        );

        drop_sessions(&relay).await;
        let missed = mail(&node, &trade, now(), "missed").await;
        local.add_event(missed.clone()).await.unwrap();

        let after = watch.next(Some(Instant::now() + OBSERVE)).await.unwrap();
        assert_eq!(ids(&after), [missed.id], "once; the window is not repeated");
        assert!(!after.truncated);
        let quiet = watch.next(Some(Instant::now() + QUIET)).await.unwrap();
        assert!(quiet.items.is_empty());
        assert_eq!(relay.connections(), 2);
        assert!(watch.close().await.is_empty());
    }

    #[tokio::test]
    async fn a_chat_follow_while_a_relay_is_down_reaches_it_after_reconnect() {
        let (node, chat) = (Keys::generate(), Chat::new());
        let base = now();
        let paid = chat.envelope(&Chat::note(&chat.them, "paid", base - 500), base - 500);
        let (away, here) = (LocalRelay::new(), LocalRelay::new());
        let (down, up) = (kickable(&away, &[]).await, serve(&here, &[&paid]).await);
        let config = config(&node, &[&down.url, &up.url]);
        let limits = WatchLimits {
            retry: Duration::from_secs(2),
            ..limits()
        };
        let start = Cursor::start(base - 1_800);
        let opened = Watch::open(&config, (Some(1), &chat.trade), None, &start, &limits).await;
        let mut watch = opened.unwrap();
        assert!(watch
            .next(Some(Instant::now()))
            .await
            .unwrap()
            .items
            .is_empty());

        drop_sessions(&down).await;
        let seen_going = watch.next(Some(Instant::now() + QUIET / 2)).await.unwrap();
        assert!(seen_going.items.is_empty());
        let started = Instant::now();
        watch.follow_chat(&chat.them.public_key()).await.unwrap();
        assert!(
            started.elapsed() < limits.read.read,
            "no wait for the relay away"
        );
        let late = Chat::note(&chat.them, "on my way", now());
        away.add_event(chat.envelope(&late, now())).await.unwrap();

        let replayed = watch.next(Some(Instant::now())).await.unwrap();
        assert_eq!(texts(&replayed), ["paid"], "the relay that stayed");
        let arrived = watch.next(Some(Instant::now() + OBSERVE)).await.unwrap();
        assert_eq!(texts(&arrived), ["on my way"], "after the reconnect");
        assert!(watch.close().await.is_empty());
    }

    #[tokio::test]
    async fn a_reconnect_does_not_extend_the_chat_replay() {
        let (node, chat) = (Keys::generate(), Chat::new());
        let stalling = || {
            LocalRelay::builder()
                .query_policy(StallLater::default())
                .build()
        };
        let (stays, comes_back) = (
            serve(&stalling(), &[]).await,
            kickable(&stalling(), &[]).await,
        );
        let config = config(&node, &[&stays.url, &comes_back.url]);
        let read = ReadLimits {
            read: Duration::from_millis(2_500),
            ..limits().read
        };
        let limits = WatchLimits { read, ..limits() };
        let start = Cursor::start(now() - 1_800);
        let opened = Watch::open(&config, (Some(1), &chat.trade), None, &start, &limits).await;
        let mut watch = opened.unwrap();

        let kick = tokio::spawn(async move {
            tokio::time::sleep(QUIET / 2).await;
            drop_sessions(&comes_back).await;
            comes_back
        });
        let started = Instant::now();
        watch.follow_chat(&chat.them.public_key()).await.unwrap();
        let elapsed = started.elapsed();

        let comes_back = kick.await.unwrap();
        assert_eq!(comes_back.connections(), 2, "back during the replay");
        assert!(
            elapsed < read.read + QUIET,
            "{elapsed:?}: the reconnect replay is not waited for"
        );
        watch.close().await;
    }

    #[tokio::test]
    async fn a_closed_subscription_is_sent_again_on_reconnect() {
        let refusal = RefuseSecond::with(MachineReadablePrefix::Restricted);
        closed_then_reconnected(refusal).await;
    }

    #[tokio::test]
    async fn a_rate_limited_subscription_is_sent_again_once() {
        let refusal = RefuseSecond::with(MachineReadablePrefix::RateLimited);
        let reqs = refusal.reqs.clone();
        closed_then_reconnected(refusal).await;
        assert_eq!(
            reqs.load(Ordering::SeqCst),
            4,
            "mail and chat, then each once again; the SDK forgot the closed id"
        );
    }

    /// The chat REQ is refused while connected, and served after a reconnect.
    async fn closed_then_reconnected(refusal: RefuseSecond) {
        let (node, chat) = (Keys::generate(), Chat::new());
        let base = now();
        let local = LocalRelay::builder().query_policy(refusal).build();
        let relay = kickable(&local, &[]).await;
        let config = config(&node, &[&relay.url]);
        let mut watch = watching(&config, &chat.trade, None, base - 1_800).await;
        assert!(watch
            .next(Some(Instant::now()))
            .await
            .unwrap()
            .items
            .is_empty());

        watch.follow_chat(&chat.them.public_key()).await.unwrap();
        let paid = chat.envelope(&Chat::note(&chat.them, "paid", base - 100), base - 100);
        local.add_event(paid).await.unwrap();
        let refused = watch.next(Some(Instant::now() + QUIET)).await.unwrap();
        assert!(refused.items.is_empty(), "not asked again while connected");

        drop_sessions(&relay).await;
        let arrived = watch.next(Some(Instant::now() + OBSERVE)).await.unwrap();
        assert_eq!(texts(&arrived), ["paid"]);
        let quiet = watch.next(Some(Instant::now() + QUIET)).await.unwrap();
        assert!(quiet.items.is_empty());
        assert!(watch.close().await.is_empty());
    }

    #[tokio::test]
    async fn a_reconnect_replay_of_fifty_new_events_sets_truncated() {
        let (node, trade) = (Keys::generate(), Keys::generate());
        let base = now();
        let local = LocalRelay::new();
        let relay = kickable(&local, &[]).await;
        let config = config(&node, &[&relay.url]);
        let mut watch = watching(&config, &trade, None, base - 1_800).await;
        assert!(watch
            .next(Some(Instant::now()))
            .await
            .unwrap()
            .items
            .is_empty());

        drop_sessions(&relay).await;
        for n in 0..51 {
            let missed = mail(&node, &trade, base - 1_000 + n, "missed").await;
            local.add_event(missed).await.unwrap();
        }

        let after = watch.next(Some(Instant::now() + OBSERVE)).await.unwrap();
        assert_eq!(after.items.len(), 50, "the relay's limit");
        assert!(after.truncated, "the oldest is missing");
        watch.close().await;
    }
}

/// How long a test waits for a live event.
const OBSERVE: Duration = Duration::from_secs(3);
/// How long a test watches to see that nothing more comes.
const QUIET: Duration = Duration::from_millis(500);

mod operation {
    use super::*;
    use chrono::Utc;
    use fake_mostro::{mail_trade_keys, seed_identity, Traded};
    use harness::Sandbox;
    use mostro_client::commands::order::watch::{order_watch, Follow, Watched};
    use mostro_client::outcome::messages::NextStep;
    use mostro_client::outcome::schema::output_schema;
    use mostro_client::outcome::watch::{watch_rules, WatchData, WatchInput};
    use mostro_client::store::{Role, Store, Trade};
    use mostro_core::prelude::{Action, Message};
    use serde_json::{json, Value};

    const OTHER: Uuid = Uuid::from_u128(0x07e4);
    const UNKNOWN: Uuid = Uuid::from_u128(0xdead);
    const NOTHING: [&str; 3] = ["not_applicable"; 3];

    /// A user holding `ORDER` as the seller (`Role::Maker` of a sell order)
    /// or the buyer (`Role::Taker`), with the conversation's keys and one
    /// loopback relay.
    pub(super) struct Desk {
        pub(super) sandbox: Sandbox,
        store: Store,
        pub(super) node: Keys,
        pub(super) chat: Chat,
        local: LocalRelay,
        pub(super) relay: LoopbackRelay,
    }

    impl Desk {
        /// The row stores the counterparty's key when `peer_stored`.
        pub(super) async fn new(role: Role, peer_stored: bool, events: &[&Event]) -> Self {
            Self::with(role, peer_stored, Chat::new(), Keys::generate(), events).await
        }

        pub(super) async fn with(
            role: Role,
            peer_stored: bool,
            chat: Chat,
            node: Keys,
            events: &[&Event],
        ) -> Self {
            let sandbox = Sandbox::new().unwrap();
            seed_identity(&sandbox.mcli_dir()).await;
            let store = Store::in_dir(&sandbox.mcli_dir());
            let (ours, theirs) = (chat.trade.public_key(), chat.them.public_key());
            let traded = match role {
                Role::Maker => Traded::active(ORDER, ours, theirs),
                Role::Taker => Traded::active(ORDER, theirs, ours),
            };
            let keys = Trade {
                index: 1,
                keys: chat.trade.clone(),
            };
            store
                .save_order_as(&traded.small(), &keys, 1, role)
                .await
                .unwrap();
            if peer_stored {
                store.set_counterparty(ORDER, &theirs).await.unwrap();
            }
            let local = LocalRelay::new();
            let relay = serve(&local, events).await;
            Self {
                sandbox,
                store,
                node,
                chat,
                local,
                relay,
            }
        }

        fn db(&self) -> std::path::PathBuf {
            self.sandbox.mcli_dir().join("mcli.db")
        }

        /// `message` from the node to our trade key at `at`.
        pub(super) async fn mail(&self, message: &Message, at: u64) -> Event {
            node_event(
                &self.node,
                self.chat.trade.public_key(),
                message,
                Timestamp::from(at),
            )
            .await
        }

        /// The end of a trade and its `rate`, stamped in the same second, with
        /// event ids that would list the `rate` first.
        pub(super) async fn end_and_rate(&self, end: Action) -> (Event, Event) {
            let at = now();
            let over = self.mail(&about(ORDER, end), at).await;
            loop {
                let rate = self.mail(&about(ORDER, Action::Rate), at).await;
                if rate.id < over.id {
                    return (over, rate);
                }
            }
        }

        /// A message `writer` wrote at `at`, in an envelope of the same time.
        pub(super) fn line(&self, writer: &Keys, text: &str, at: u64) -> Event {
            self.chat.envelope(&Chat::note(writer, text, at), at)
        }

        /// Put `event` on the relay after `delay`, while a watch runs.
        pub(super) fn publish(&self, event: Event, delay: Duration) {
            let local = self.local.clone();
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                local.add_event(event).await.unwrap();
            });
        }

        async fn watch(
            &self,
            order: Uuid,
            after: Option<&str>,
            timeout: i64,
            follow: Follow<'_>,
        ) -> Watched {
            let json = matches!(follow, Follow::FirstChange);
            let input = WatchInput::validate(order, None, after, Some(timeout), json).unwrap();
            let config = config(&self.node, &[&self.relay.url]);
            order_watch(&input, &config, &self.store, &limits(), Utc::now(), follow).await
        }

        /// A JSON call, as its outcome.
        async fn json(&self, after: Option<&str>, timeout: i64) -> Value {
            let watched = self.watch(ORDER, after, timeout, Follow::FirstChange).await;
            serde_json::to_value(&watched.outcome).unwrap()
        }

        /// A human watch: every update, and how it ended.
        async fn follow(&self, timeout: i64) -> (Vec<WatchData>, Watched) {
            let mut updates = Vec::new();
            let mut sink = |update: &WatchData| {
                updates.push(update.clone());
                true
            };
            let watched = self
                .watch(ORDER, None, timeout, Follow::UntilEnd(&mut sink))
                .await;
            (updates, watched)
        }
    }

    pub(super) fn about(order: Uuid, action: Action) -> Message {
        Message::new_order(Some(order), None, None, action, None)
    }

    /// The status, error code and stages of an outcome with no snapshot.
    fn result(outcome: &Value) -> (&str, Option<&str>, [&str; 3]) {
        let meta = &outcome["meta"];
        let stage = |name: &str| meta[name].as_str().unwrap();
        assert_eq!(meta["snapshot"], Value::Null);
        (
            outcome["status"].as_str().unwrap(),
            outcome["error"]["code"].as_str(),
            [
                stage("publication"),
                stage("daemon_acceptance"),
                stage("persistence"),
            ],
        )
    }

    fn event_ids(data: &Value) -> Vec<String> {
        data["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["event_id"].as_str().unwrap().to_string())
            .collect()
    }

    fn chat_texts(data: &Value) -> Vec<String> {
        data["chat"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line["text"].as_str().unwrap().to_string())
            .collect()
    }

    fn actions(updates: &[WatchData]) -> Vec<String> {
        updates
            .iter()
            .flat_map(|update| update.messages().iter().map(|m| m.action().to_string()))
            .collect()
    }

    fn cursor_since(data: &Value) -> u64 {
        let cursor = data["cursor"].as_str().unwrap();
        cursor.split(':').next().unwrap().parse().unwrap()
    }

    #[tokio::test]
    async fn json_mode_returns_the_replay_then_only_newer_records() {
        let base = now();
        let (chat, node) = (Chat::new(), Keys::generate());
        let older = node_event(
            &node,
            chat.trade.public_key(),
            &mail_text(ORDER, "one"),
            Timestamp::from(base - 600),
        )
        .await;
        let newest = node_event(
            &node,
            chat.trade.public_key(),
            &mail_text(ORDER, "two"),
            Timestamp::from(base - 10),
        )
        .await;
        let desk = Desk::with(Role::Taker, true, chat, node, &[&older, &newest]).await;

        let first = desk.json(None, 2).await;
        assert_eq!(result(&first), ("ok", None, NOTHING));
        let data = &first["data"];
        assert_eq!(event_ids(data), [older.id.to_hex(), newest.id.to_hex()]);
        assert_eq!(
            data["counterparty_pubkey"],
            desk.chat.them.public_key().to_hex()
        );
        assert_eq!(
            (&data["ended"], &data["truncated"]),
            (&json!(false), &json!(false))
        );
        let cursor = data["cursor"].as_str().unwrap().to_string();
        assert!(cursor.contains(&newest.id.to_hex()), "{cursor}");

        // Stamped in the same second as the newest, published while it waits.
        let sibling = desk.mail(&mail_text(ORDER, "sibling"), base - 10).await;
        desk.publish(sibling.clone(), QUIET);
        let second = desk.json(Some(&cursor), 5).await;
        assert_eq!(result(&second), ("ok", None, NOTHING));
        assert_eq!(event_ids(&second["data"]), [sibling.id.to_hex()]);
        assert!(chat_texts(&second["data"]).is_empty());
    }

    #[tokio::test]
    async fn a_chat_line_stamped_before_the_newest_record_is_not_lost() {
        let base = now();
        let (chat, node) = (Chat::new(), Keys::generate());
        let newest = node_event(
            &node,
            chat.trade.public_key(),
            &mail_text(ORDER, "fiat sent"),
            Timestamp::from(base - 10),
        )
        .await;
        let desk = Desk::with(Role::Taker, true, chat, node, &[&newest]).await;
        let first = desk.json(None, 2).await;
        assert_eq!(event_ids(&first["data"]), [newest.id.to_hex()]);
        let cursor = first["data"]["cursor"].as_str().unwrap().to_string();

        // The counterparty's clock is 30 s behind.
        let late = desk.line(&desk.chat.them, "received", base - 40);
        desk.publish(late, QUIET);
        let second = desk.json(Some(&cursor), 5).await;
        assert_eq!(chat_texts(&second["data"]), ["received"]);
        assert!(event_ids(&second["data"]).is_empty());
    }

    #[tokio::test]
    async fn a_future_stamped_chat_line_does_not_push_the_cursor_past_now() {
        let base = now();
        let chat = Chat::new();
        let ahead = chat.envelope(
            &Chat::note(&chat.them, "from the future", base + 50),
            base + 50,
        );
        let desk = Desk::with(Role::Taker, true, chat, Keys::generate(), &[&ahead]).await;
        let first = desk.json(None, 2).await;
        assert_eq!(chat_texts(&first["data"]), ["from the future"]);
        assert!(cursor_since(&first["data"]) <= now() - 60);
        let cursor = first["data"]["cursor"].as_str().unwrap().to_string();

        // Stamped before the chat line, it reaches the relays later.
        let slow = desk.mail(&mail_text(ORDER, "slow"), base - 5).await;
        desk.publish(slow.clone(), QUIET);
        let second = desk.json(Some(&cursor), 5).await;
        assert_eq!(event_ids(&second["data"]), [slow.id.to_hex()]);
        assert!(chat_texts(&second["data"]).is_empty(), "not again");
    }

    #[tokio::test]
    async fn the_counterparty_learned_while_watching_brings_its_chat_in_the_same_document() {
        let base = now();
        let chat = Chat::new();
        let paid = chat.envelope(&Chat::note(&chat.them, "paid", base - 100), base - 100);
        let desk = Desk::with(Role::Taker, false, chat, Keys::generate(), &[&paid]).await;
        let (ours, theirs) = (desk.chat.trade.public_key(), desk.chat.them.public_key());
        let accepted = mail_trade_keys(Action::HoldInvoicePaymentAccepted, ORDER, ours, theirs);
        let lock = desk.mail(&accepted, now()).await;
        desk.publish(lock.clone(), QUIET);

        let outcome = desk.json(None, 5).await;

        assert_eq!(result(&outcome), ("ok", None, NOTHING));
        let data = &outcome["data"];
        assert_eq!(data["counterparty_pubkey"], theirs.to_hex());
        assert_eq!(event_ids(data), [lock.id.to_hex()]);
        assert_eq!(
            chat_texts(data),
            ["paid"],
            "the chat replay, in the same document"
        );
        assert_eq!(
            desk.store.counterparty(ORDER).await.unwrap(),
            None,
            "not saved"
        );
    }

    #[tokio::test]
    async fn the_end_of_the_trade_is_reported_and_ends_the_follow() {
        let desk = Desk::new(Role::Taker, true, &[]).await;
        let other = desk.mail(&about(OTHER, Action::Canceled), now()).await;
        let completed = desk
            .mail(&about(ORDER, Action::PurchaseCompleted), now())
            .await;
        desk.publish(other, QUIET);
        desk.publish(completed, QUIET * 3);

        let (updates, watched) = desk.follow(10).await;

        assert_eq!(actions(&updates), ["canceled", "purchase-completed"]);
        assert!(
            updates[0].messages().is_empty(),
            "the first update is the empty replay"
        );
        let ended: Vec<bool> = updates.iter().map(WatchData::ended).collect();
        assert_eq!(ended.last(), Some(&true));
        assert!(!ended[..ended.len() - 1].contains(&true), "{ended:?}");
        assert_eq!(watched.ended_by.as_deref(), Some("purchase-completed"));
        let outcome = serde_json::to_value(&watched.outcome).unwrap();
        assert_eq!(result(&outcome), ("ok", None, NOTHING));
        assert_eq!(outcome["data"]["ended"], json!(true));

        // An end already in the replay.
        let base = now();
        let (chat, node) = (Chat::new(), Keys::generate());
        let canceled = node_event(
            &node,
            chat.trade.public_key(),
            &about(ORDER, Action::Canceled),
            Timestamp::from(base - 100),
        )
        .await;
        let desk = Desk::with(Role::Taker, true, chat, node, &[&canceled]).await;
        let first = desk.json(None, 2).await;
        assert_eq!(first["data"]["ended"], json!(true));
        let (updates, watched) = desk.follow(10).await;
        assert_eq!(updates.len(), 1, "it ends after the first update");
        assert_eq!(watched.ended_by.as_deref(), Some("canceled"));
    }

    #[tokio::test]
    async fn admin_settled_ends_the_seller_but_not_the_buyer() {
        let base = now();
        let settled = |chat: &Chat, node: &Keys| {
            let (to, node) = (chat.trade.public_key(), node.clone());
            async move {
                node_event(
                    &node,
                    to,
                    &about(ORDER, Action::AdminSettled),
                    Timestamp::from(base - 100),
                )
                .await
            }
        };
        let (chat, node) = (Chat::new(), Keys::generate());
        let event = settled(&chat, &node).await;
        let seller = Desk::with(Role::Maker, true, chat, node, &[&event]).await;
        let (updates, watched) = seller.follow(10).await;
        assert_eq!(updates.len(), 1);
        assert_eq!(watched.ended_by.as_deref(), Some("admin-settled"));

        let (chat, node) = (Chat::new(), Keys::generate());
        let event = settled(&chat, &node).await;
        let buyer = Desk::with(Role::Taker, true, chat, node, &[&event]).await;
        let failed = buyer
            .mail(&about(ORDER, Action::PaymentFailed), now())
            .await;
        let completed = buyer
            .mail(&about(ORDER, Action::PurchaseCompleted), now())
            .await;
        buyer.publish(failed, QUIET);
        buyer.publish(completed, QUIET * 3);
        let (updates, watched) = buyer.follow(10).await;
        assert_eq!(
            actions(&updates),
            ["admin-settled", "payment-failed", "purchase-completed"]
        );
        assert_eq!(watched.ended_by.as_deref(), Some("purchase-completed"));
    }

    #[tokio::test]
    async fn a_watch_ends_after_the_rate_that_follows_the_end_of_the_trade() {
        let ends = [
            (
                Role::Maker,
                Action::HoldInvoicePaymentSettled,
                "hold-invoice-payment-settled",
            ),
            (Role::Taker, Action::PurchaseCompleted, "purchase-completed"),
        ];
        for (role, end, name) in ends {
            let desk = Desk::new(role, true, &[]).await;
            let (over, rate) = desk.end_and_rate(end).await;
            desk.publish(over, QUIET);
            desk.publish(rate, QUIET * 2);

            let (updates, watched) = desk.follow(10).await;

            assert_eq!(actions(&updates), [name, "rate"]);
            let last = updates.last().unwrap();
            assert!(last.ended(), "{name}");
            assert!(
                !updates[..updates.len() - 1].iter().any(WatchData::ended),
                "{name}"
            );
            let asked = last.messages().last().unwrap().next_step();
            assert_eq!(asked, Some(NextStep::Rate), "{name}");
            assert_eq!(watched.ended_by.as_deref(), Some(name));
        }

        // A JSON call returns the end and its rate in one document.
        let desk = Desk::new(Role::Taker, true, &[]).await;
        let (over, rate) = desk.end_and_rate(Action::PurchaseCompleted).await;
        desk.publish(over.clone(), QUIET);
        desk.publish(rate.clone(), QUIET * 2);
        let outcome = desk.json(None, 10).await;
        let data = &outcome["data"];
        assert_eq!(event_ids(data), [over.id.to_hex(), rate.id.to_hex()]);
        assert_eq!(data["ended"], json!(true));

        // No rate: the grace window ends the watch, long before its deadline.
        let desk = Desk::new(Role::Taker, true, &[]).await;
        let over = desk
            .mail(&about(ORDER, Action::PurchaseCompleted), now())
            .await;
        desk.publish(over, QUIET);
        let started = Instant::now();
        let (updates, watched) = desk.follow(30).await;
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(actions(&updates), ["purchase-completed"]);
        assert!(updates.last().unwrap().ended());
        assert_eq!(watched.ended_by.as_deref(), Some("purchase-completed"));
    }

    #[tokio::test]
    async fn nothing_new_until_the_deadline_is_an_empty_success_in_both_modes() {
        let desk = Desk::new(Role::Taker, true, &[]).await;
        let before = now() - 1_800;
        let outcome = desk.json(None, 1).await;
        assert_eq!(result(&outcome), ("ok", None, NOTHING));
        let data = &outcome["data"];
        assert!(event_ids(data).is_empty() && chat_texts(data).is_empty());
        assert_eq!(data["ended"], json!(false));
        let cursor = data["cursor"].as_str().unwrap();
        assert!(!cursor.contains(':'), "the start, carried: {cursor}");
        assert!((before..=now() - 1_800).contains(&cursor_since(data)));

        let (updates, watched) = desk.follow(1).await;
        assert_eq!(updates.len(), 1, "only the first update");
        assert_eq!(watched.ended_by, None);
        let outcome = serde_json::to_value(&watched.outcome).unwrap();
        assert_eq!(result(&outcome), ("ok", None, NOTHING));
        assert_eq!(outcome["data"]["ended"], json!(false));

        // A sink that cannot write stops the watch at once.
        let started = Instant::now();
        let mut refused = 0;
        let mut sink = |_: &WatchData| {
            refused += 1;
            false
        };
        let watched = desk
            .watch(ORDER, None, 60, Follow::UntilEnd(&mut sink))
            .await;
        assert_eq!(refused, 1);
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(watched.ended_by, None);
    }

    #[tokio::test]
    async fn unknown_order_or_store_fail_before_any_relay() {
        let desk = Desk::new(Role::Taker, true, &[]).await;
        let silent = LoopbackRelay::start(Backend::Silent).await.unwrap();
        let input = WatchInput::validate(UNKNOWN, None, None, Some(1), true).unwrap();
        let config = config(&desk.node, &[&silent.url]);
        let code = |watched: Watched| {
            let outcome = serde_json::to_value(&watched.outcome).unwrap();
            assert_eq!(outcome["data"], Value::Null);
            outcome["error"]["code"].as_str().unwrap().to_string()
        };
        let run = |input: WatchInput| {
            let (config, store) = (config.clone(), &desk.store);
            async move {
                order_watch(
                    &input,
                    &config,
                    store,
                    &limits(),
                    Utc::now(),
                    Follow::FirstChange,
                )
                .await
            }
        };
        assert_eq!(code(run(input).await), "order_unknown");
        let input = WatchInput::validate(ORDER, None, None, Some(1), true).unwrap();
        std::fs::remove_file(desk.db()).unwrap();
        assert_eq!(code(run(input.clone()).await), "identity_missing");
        std::fs::create_dir(desk.db()).unwrap();
        assert_eq!(code(run(input).await), "store_unreadable");
        assert_eq!(silent.connections(), 0, "no relay was contacted");
    }

    #[tokio::test]
    async fn a_lookup_failure_keeps_its_code() {
        let desk = Desk::new(Role::Taker, false, &[]).await;
        let input = WatchInput::validate(ORDER, None, None, Some(1), true).unwrap();
        let config = config(&desk.node, &[&dead_relay()]);
        let watched = order_watch(
            &input,
            &config,
            &desk.store,
            &limits(),
            Utc::now(),
            Follow::FirstChange,
        )
        .await;
        let outcome = serde_json::to_value(&watched.outcome).unwrap();
        assert_eq!(
            result(&outcome),
            ("error", Some("relay_connection_failed"), NOTHING)
        );
    }

    #[tokio::test]
    async fn a_frozen_store_still_watches_and_conforms() {
        let base = now();
        let (chat, node) = (Chat::new(), Keys::generate());
        let (ours, theirs) = (chat.trade.public_key(), chat.them.public_key());
        let accepted = mail_trade_keys(Action::HoldInvoicePaymentAccepted, ORDER, ours, theirs);
        let lock = node_event(&node, ours, &accepted, Timestamp::from(base - 100)).await;
        let desk = Desk::with(Role::Taker, false, chat, node, &[&lock]).await;
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(desk.db());
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
             BEGIN SELECT RAISE(ABORT, 'frozen'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let outcome = desk.json(None, 2).await;

        assert_eq!(result(&outcome), ("ok", None, NOTHING));
        assert_eq!(
            outcome["warnings"],
            json!([]),
            "the failed cache write is not reported"
        );
        assert_eq!(outcome["data"]["counterparty_pubkey"], theirs.to_hex());
        assert_eq!(desk.store.counterparty(ORDER).await.unwrap(), None);
        let schema = Value::Object(output_schema::<WatchData, ()>(&watch_rules()));
        assert!(
            schema_check::conforms(&schema, &schema, &outcome),
            "{outcome}"
        );
    }
}

mod binary {
    use super::operation::{about, Desk};
    use super::*;
    use harness::{closed_pipe, run_with_watchdog, Captured, SENTINEL, WATCHDOG};
    use mostro_client::store::Role;
    use mostro_core::prelude::Action;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    use tokio::process::Command;
    use tokio::time::timeout;

    const DEADLINE_LINE: &str = "⏰ Stopped at the watch deadline; the trade has not ended.\n";

    /// The binary in `desk`'s sandbox, with its node and relay, watching
    /// `ORDER` with `extra` arguments after `front` ones.
    pub(super) fn command(desk: &Desk, front: &[&str], extra: &[&str]) -> Command {
        let order = ORDER.to_string();
        let mut args = front.to_vec();
        args.extend(["order", "watch", "--order", &order]);
        args.extend_from_slice(extra);
        let mut command = desk.sandbox.command(&args);
        command
            .env("MOSTRO_PUBKEY", desk.node.public_key().to_hex())
            .env("RELAYS", &desk.relay.url);
        command
    }

    pub(super) async fn run(command: Command) -> Captured {
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert!(run.reaped && run.status.is_some());
        run
    }

    pub(super) async fn quiet_desk() -> Desk {
        Desk::new(Role::Taker, true, &[]).await
    }

    #[tokio::test]
    async fn human_streams_until_the_trade_ends() {
        let base = now();
        let (chat, node) = (Chat::new(), Keys::generate());
        let to = chat.trade.public_key();
        let replayed = mail_text(ORDER, "replayed");
        let replayed = node_event(&node, to, &replayed, Timestamp::from(base - 600)).await;
        let desk = Desk::with(Role::Taker, true, chat, node, &[&replayed]).await;
        let mut child = command(&desk, &[], &[]).spawn().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut out = String::new();
        // The first update is written whole: once its header arrives, the
        // replay is over and the watch is live.
        while !out.contains("👀 Watching order") {
            let line = timeout(WATCHDOG, lines.next_line()).await.unwrap();
            out.push_str(&line.unwrap().expect("the header"));
            out.push('\n');
        }
        let live = desk.mail(&mail_text(ORDER, "live"), now()).await;
        let forged = "paid\u{1b}[2J\u{202e}gnissim";
        let line = desk.line(&desk.chat.them, forged, now());
        let completed = desk
            .mail(&about(ORDER, Action::PurchaseCompleted), now() - 1)
            .await;
        desk.publish(live, QUIET);
        desk.publish(line, QUIET * 2);
        desk.publish(completed, QUIET * 3);
        let rate = desk.mail(&about(ORDER, Action::Rate), now()).await;
        desk.publish(rate, QUIET * 4);

        while let Some(line) = timeout(WATCHDOG, lines.next_line()).await.unwrap().unwrap() {
            out.push_str(&line);
            out.push('\n');
        }
        let status = timeout(WATCHDOG, child.wait()).await.unwrap().unwrap();
        let mut err = String::new();
        stderr.read_to_string(&mut err).await.unwrap();

        assert_eq!(status.code(), Some(0), "{err}");
        assert!(err.is_empty(), "{err}");
        let at = |text: &str| out.find(text).unwrap_or_else(|| panic!("{text}\n{out}"));
        assert!(at("👀 Watching order") < at("replayed"), "{out}");
        assert!(at("replayed") < at("live"), "{out}");
        assert!(at("live") < at("💬 Chat:"), "{out}");
        assert!(out.contains("   paid\\u{1b}[2J\\u{202e}gnissim\n"), "{out}");
        assert!(
            !out.contains('\u{1b}') && !out.contains('\u{202e}'),
            "{out}"
        );
        assert!(
            out.ends_with("🏁 The trade ended: purchase-completed\n"),
            "{out}"
        );
        let rating = format!("👉 Next: rate your counterparty: `order rate --order {ORDER} ");
        assert!(at("purchase-completed") < at(&rating), "{out}");
        let secret = desk.chat.trade.secret_key().to_secret_hex();
        assert!(!out.contains(&secret) && !err.contains(&secret));
    }

    #[tokio::test]
    async fn human_deadline_prints_the_final_line_and_exits_0() {
        let desk = quiet_desk().await;

        let run = run(command(&desk, &[], &["--timeout", "1"])).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let out = run.stdout_text();
        assert!(out.contains("📭 Nothing yet;"), "{out}");
        assert!(out.ends_with(DEADLINE_LINE), "{out}");
    }

    #[tokio::test]
    async fn a_closed_stdout_exits_1_with_one_fixed_line() {
        let desk = quiet_desk().await;
        let mut command = command(&desk, &[], &["--timeout", "30"]);
        command.stdout(closed_pipe());
        let started = Instant::now();

        let run = run(command).await;

        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        assert_eq!(run.stderr_text(), "error: the watch could not be written\n");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "not the deadline"
        );
    }

    #[tokio::test]
    async fn bad_arguments_exit_2_before_any_relay() {
        let desk = quiet_desk().await;
        let bad: [&[&str]; 5] = [
            &["--since", "0"],
            &["--since", "10081"],
            &["--timeout", "0"],
            &["--after", SENTINEL],
            &["--after", "100", "--since", "5"],
        ];
        for extra in bad {
            let run = run(command(&desk, &[], extra)).await;

            assert_eq!(run.code(), Some(2), "{extra:?}: {}", run.stderr_text());
            assert!(run.stdout.is_empty(), "{extra:?}");
            assert!(!run.stderr_text().contains(SENTINEL), "{extra:?}");
        }
        assert_eq!(desk.relay.connections(), 0, "no relay was contacted");
    }

    #[tokio::test]
    async fn verbose_prints_one_line() {
        let desk = quiet_desk().await;

        let run = run(command(&desk, &["--verbose"], &["--timeout", "1"])).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(run.stderr_text(), "verbose: order.watch ok\n");
    }

    #[tokio::test]
    async fn a_bad_pow_or_transport_setting_does_not_change_order_watch() {
        let desk = quiet_desk().await;
        for (front, set) in [
            (&["--pow", SENTINEL][..], &[][..]),
            (&["--transport", SENTINEL][..], &[][..]),
            (&[][..], &[("POW", SENTINEL), ("TRANSPORT", SENTINEL)][..]),
        ] {
            let mut command = command(&desk, front, &["--timeout", "1"]);
            command.envs(set.iter().copied());

            let run = run(command).await;

            assert_eq!(run.code(), Some(0), "{front:?}: {}", run.stderr_text());
            assert!(run.stdout_text().ends_with(DEADLINE_LINE), "{front:?}");
        }
    }
}

mod json {
    use super::binary::{command, quiet_desk, run};
    use super::operation::Desk;
    use super::*;
    use harness::{Captured, SENTINEL};
    use mostro_client::store::Role;
    use serde_json::{json, Value};

    const JSON: [&str; 2] = ["--output", "json"];

    /// The checked-in `order.watch` output schema.
    fn schema() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/schemas/order-watch-v1.json"
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The one document of a run, checked against the schema.
    fn document(run: &Captured) -> Value {
        let out = run.stdout_text();
        assert_eq!(out.lines().count(), 1, "{out}");
        let outcome: Value = serde_json::from_str(&out).unwrap();
        let schema = schema();
        assert!(schema_check::conforms(&schema, &schema, &outcome), "{out}");
        outcome
    }

    fn texts(outcome: &Value) -> Vec<String> {
        let messages = outcome["data"]["messages"].as_array().unwrap();
        messages
            .iter()
            .map(|message| message["payload"]["text"].as_str().unwrap().into())
            .collect()
    }

    #[tokio::test]
    async fn json_returns_one_document_at_the_first_new_record() {
        let (chat, node) = (Chat::new(), Keys::generate());
        let to = chat.trade.public_key();
        let replayed = mail_text(ORDER, "replayed");
        let replayed = node_event(&node, to, &replayed, Timestamp::from(now() - 600)).await;
        let desk = Desk::with(Role::Taker, true, chat, node, &[&replayed]).await;

        let first = run(command(&desk, &JSON, &[])).await;

        assert_eq!(first.code(), Some(0), "{}", first.stderr_text());
        assert!(first.stderr.is_empty(), "{}", first.stderr_text());
        let first = document(&first);
        assert_eq!(first["status"], "ok");
        assert_eq!(texts(&first), ["replayed"]);
        assert_eq!(first["data"]["ended"], false);
        let cursor = first["data"]["cursor"].as_str().unwrap().to_string();

        let live = desk.mail(&mail_text(ORDER, "live"), now()).await;
        desk.publish(live, QUIET);
        let next = run(command(&desk, &JSON, &["--after", &cursor])).await;

        assert_eq!(next.code(), Some(0), "{}", next.stderr_text());
        let next = document(&next);
        assert_eq!(texts(&next), ["live"], "only the record after the cursor");
        assert_ne!(next["data"]["cursor"], json!(cursor));
    }

    #[tokio::test]
    async fn json_timeout_is_an_empty_success_with_the_given_cursor() {
        let desk = quiet_desk().await;
        let cursor = (now() - 60).to_string();
        let started = Instant::now();

        let run = run(command(
            &desk,
            &JSON,
            &["--after", &cursor, "--timeout", "1"],
        ))
        .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        assert!(started.elapsed() < Duration::from_secs(15), "bounded");
        let outcome = document(&run);
        let data = &outcome["data"];
        assert_eq!(outcome["status"], "ok");
        assert_eq!((&data["messages"], &data["chat"]), (&json!([]), &json!([])));
        assert_eq!(
            (&data["ended"], &data["cursor"]),
            (&json!(false), &json!(cursor))
        );
    }

    #[tokio::test]
    async fn json_errors_conform_with_null_data() {
        let desk = quiet_desk().await;
        let unknown = Uuid::from_u128(0xdead).to_string();
        let mut args = JSON.to_vec();
        args.extend(["order", "watch", "--order", &unknown, "--timeout", "1"]);
        let mut unknown = desk.sandbox.command(&args);
        unknown
            .env("MOSTRO_PUBKEY", desk.node.public_key().to_hex())
            .env("RELAYS", &desk.relay.url);
        let cases = [
            (run(unknown).await, 1, "order_unknown"),
            (
                run(command(&desk, &JSON, &["--after", SENTINEL])).await,
                2,
                "invalid_arguments",
            ),
        ];

        for (run, exit, code) in cases {
            assert_eq!(run.code(), Some(exit), "{code}: {}", run.stderr_text());
            assert!(run.stderr.is_empty(), "{code}: {}", run.stderr_text());
            assert!(!run.stdout_text().contains(SENTINEL), "{code}");
            let outcome = document(&run);
            assert_eq!(outcome["status"], "error", "{code}");
            assert_eq!(outcome["error"]["code"], code);
            assert_eq!(outcome["data"], Value::Null, "{code}");
        }
        assert_eq!(desk.relay.connections(), 0, "no relay was contacted");
    }
}
