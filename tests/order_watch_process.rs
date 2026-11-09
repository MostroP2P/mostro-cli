//! Tests for `order watch` (spec 020): the live subscription (units 2 and 3).
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
