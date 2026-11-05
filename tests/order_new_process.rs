//! Tests for `order new` (spec 003).
//!
//! Slice 1 holds the characterization of what the legacy `neworder` puts on the
//! wire and the self-tests of the fixtures built for the command: the scripted
//! daemon that applies the source's `check_trade_index` and `order_action`
//! rules, and the writable identity store. The command's own scenarios are
//! added with the slices that implement it.

use std::time::Duration;

use futures::StreamExt;
use mostro_client::msg::{exchange, ExchangeLimits, Expect, Outbound, PublicReadConfig};
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, Payload, Transport,
    WrapOptions,
};
use nostr_sdk::prelude::{
    Client, ClientNotification, Event, Filter, Keys, Kind, LocalRelay, MachineReadablePrefix,
    PublicKey, WritePolicy, WritePolicyResult,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::time::timeout;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{
    info_event, new_order_daemon, seed_identity, set_last_trade_index, FakeMostro, Respond,
};
use harness::{Backend, LoopbackRelay, Sandbox};

const REQUEST_ID: u64 = 4410;

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

/// The order `execute_new_order` builds (`cli/new_order.rs`): kind and status
/// set, `id` and `creator`-side fields empty, a fixed amount in `fiat_amount`
/// and a range in `min`/`max` with `fiat_amount` 0.
fn legacy_order(fiat: (i64, Option<i64>), sats: i64, invoice: Option<&str>) -> SmallOrder {
    let amt = if fiat.1.is_some() {
        (0, Some(fiat.0), fiat.1)
    } else {
        (fiat.0, None, None)
    };
    SmallOrder::new(
        None,
        Some(OrderKind::Sell),
        Some(Status::Pending),
        sats,
        "USD".to_string(),
        amt.1,
        amt.2,
        amt.0,
        "bank transfer".to_string(),
        2,
        None,
        None,
        invoice.map(str::to_owned),
        Some(0),
        None,
    )
}

fn request(order: SmallOrder, index: i64) -> Message {
    Message::new_order(
        None,
        Some(REQUEST_ID),
        Some(index),
        Action::NewOrder,
        Some(Payload::Order(order)),
    )
}

async fn wrap(message: &Message, identity: &Keys, trade: &Keys, node: PublicKey) -> Event {
    wrap_message_with(
        Transport::Nip44Direct,
        message,
        identity,
        trade,
        node,
        WrapOptions::default(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn the_legacy_request_is_signed_by_the_trade_key_under_the_identity_seal() {
    let (identity, trade, node) = (Keys::generate(), Keys::generate(), Keys::generate());
    let message = request(legacy_order((100, None), 0, None), 3);

    let event = wrap(&message, &identity, &trade, node.public_key()).await;

    assert_eq!(event.kind, Kind::PrivateDirectMessage);
    let seen = unwrap_incoming(&event, &node).await.unwrap().unwrap();
    assert_eq!(
        seen.sender,
        trade.public_key(),
        "the rumor author is the trade key"
    );
    assert_eq!(
        seen.identity,
        identity.public_key(),
        "the seal signer is the identity"
    );
    let signature = seen.signature.expect("signed by default");
    assert!(Message::verify_signature(
        seen.message.as_json().unwrap(),
        trade.public_key(),
        signature
    ));
    let inner = seen.message.get_inner_message_kind();
    assert_eq!(inner.action, Action::NewOrder);
    assert_eq!(inner.request_id, Some(REQUEST_ID));
    assert_eq!(inner.trade_index, Some(3));
    assert!(inner.id.is_none(), "the id is the node's to assign");
}

#[test]
fn the_legacy_payload_for_a_fixed_and_a_range_order() {
    let json_of = |order: SmallOrder| -> Value {
        let text = request(order, 1).as_json().unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        value["order"]["payload"]["order"].clone()
    };

    let fixed = json_of(legacy_order((100, None), 0, Some("lnbc1")));
    assert_eq!(fixed["kind"], "sell");
    assert_eq!(fixed["status"], "pending");
    assert_eq!(fixed["amount"], 0, "market price");
    assert_eq!(fixed["fiat_code"], "USD");
    assert_eq!(fixed["fiat_amount"], 100);
    assert_eq!(fixed["min_amount"], Value::Null);
    assert_eq!(fixed["max_amount"], Value::Null);
    assert_eq!(fixed["payment_method"], "bank transfer");
    assert_eq!(fixed["premium"], 2);
    assert_eq!(fixed["buyer_invoice"], "lnbc1");
    assert_eq!(fixed["id"], Value::Null);

    let range = json_of(legacy_order((100, Some(200)), 0, None));
    assert_eq!(range["fiat_amount"], 0, "a range carries no single amount");
    assert_eq!(range["min_amount"], 100);
    assert_eq!(range["max_amount"], 200);
}

/// A requester with a trade key distinct from the identity, listening on the
/// trade key as the real command must.
struct Maker {
    client: Client,
    identity: Keys,
    trade: Keys,
    node: PublicKey,
}

impl Maker {
    async fn connect(relay: &LoopbackRelay, node: PublicKey) -> Self {
        let client = Client::default();
        client.add_relay(relay.url.as_str()).await.unwrap();
        let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
        assert!(connected.failed.is_empty(), "{:?}", connected.failed);
        Self {
            client,
            identity: Keys::generate(),
            trade: Keys::generate(),
            node,
        }
    }

    /// Send `message` and return what the node answers to the trade key within `wait`.
    async fn send(&self, message: &Message, wait: Duration) -> Vec<Message> {
        self.send_as(message, &self.identity, &self.trade, wait)
            .await
    }

    async fn send_as(
        &self,
        message: &Message,
        identity: &Keys,
        trade: &Keys,
        wait: Duration,
    ) -> Vec<Message> {
        let mut notifications = self.client.notifications();
        let filter = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .pubkey(trade.public_key())
            .author(self.node)
            .limit(0);
        self.client.subscribe(filter).await.unwrap();
        let event = wrap(message, identity, trade, self.node).await;
        self.client.send_event(&event).await.unwrap();

        let mut replies = Vec::new();
        let _ = timeout(wait, async {
            while let Some(notification) = notifications.next().await {
                let ClientNotification::Event { event, .. } = notification else {
                    continue;
                };
                if let Ok(Some(seen)) = unwrap_incoming(&event, trade).await {
                    replies.push(seen.message);
                }
            }
        })
        .await;
        replies
    }
}

async fn node_with(respond: Respond) -> (LoopbackRelay, FakeMostro) {
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

const WAIT: Duration = Duration::from_millis(700);

fn only(replies: Vec<Message>) -> mostro_core::prelude::MessageKind {
    assert_eq!(replies.len(), 1, "{replies:?}");
    replies[0].get_inner_message_kind().clone()
}

#[tokio::test]
async fn the_scripted_node_acknowledges_an_order_to_the_trade_key_and_remembers_the_index() {
    let (respond, last) = new_order_daemon(2, false);
    let (relay, fake) = node_with(respond).await;
    let maker = Maker::connect(&relay, fake.pubkey()).await;

    let reply = only(
        maker
            .send(&request(legacy_order((100, None), 0, None), 3), WAIT)
            .await,
    );

    assert_eq!(reply.action, Action::NewOrder);
    assert_eq!(reply.request_id, Some(REQUEST_ID));
    assert_eq!(reply.trade_index, Some(3));
    let created = reply.id.expect("the node assigns the id");
    let Some(Payload::Order(order)) = reply.payload else {
        panic!("an order payload");
    };
    assert_eq!(order.id, Some(created));
    assert_eq!(order.fiat_amount, 100);
    assert_eq!(*last.lock().unwrap(), 3);
    let seen = fake.received();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].signature_valid);
    assert_eq!(seen[0].trade_index, Some(3));
    assert_eq!(seen[0].author, maker.trade.public_key());
    assert_eq!(seen[0].identity, maker.identity.public_key());
    assert_ne!(seen[0].author, seen[0].identity);
}

#[tokio::test]
async fn the_scripted_node_refuses_like_the_source_and_keeps_the_index_on_refusal() {
    let (respond, last) = new_order_daemon(2, false);
    let (relay, fake) = node_with(respond).await;
    let maker = Maker::connect(&relay, fake.pubkey()).await;
    let reason = |kind: mostro_core::prelude::MessageKind| match kind.payload {
        Some(Payload::CantDo(Some(reason))) => reason,
        other => panic!("a refusal, got {other:?}"),
    };

    // An index that is not above the node's last.
    let stale = only(
        maker
            .send(&request(legacy_order((100, None), 0, None), 2), WAIT)
            .await,
    );
    assert_eq!(stale.action, Action::CantDo);
    assert_eq!(reason(stale), CantDoReason::InvalidTradeIndex);

    // A half-specified range.
    let mut half = legacy_order((100, Some(200)), 0, None);
    half.max_amount = None;
    let half = only(maker.send(&request(half, 3), WAIT).await);
    assert_eq!(reason(half), CantDoReason::InvalidAmount);

    // A fixed order without a fiat amount.
    let zero = only(
        maker
            .send(&request(legacy_order((0, None), 0, None), 3), WAIT)
            .await,
    );
    assert_eq!(reason(zero), CantDoReason::InvalidAmount);

    // The identity used as the trade key.
    let same = Keys::generate();
    let alone = maker
        .send_as(
            &request(legacy_order((100, None), 0, None), 3),
            &same,
            &same,
            WAIT,
        )
        .await;
    assert_eq!(reason(only(alone)), CantDoReason::InvalidTradeIndex);

    assert_eq!(*last.lock().unwrap(), 2, "no refusal consumed the index");
    assert_eq!(fake.received().len(), 4);

    // The same index still works, once; then it is spent.
    let ok = only(
        maker
            .send(&request(legacy_order((100, None), 0, None), 3), WAIT)
            .await,
    );
    assert_eq!(ok.action, Action::NewOrder);
    let again = only(
        maker
            .send(&request(legacy_order((100, None), 0, None), 3), WAIT)
            .await,
    );
    assert_eq!(reason(again), CantDoReason::InvalidTradeIndex);
    assert_eq!(*last.lock().unwrap(), 3);
}

#[tokio::test]
async fn a_bonded_scripted_node_asks_for_a_bond_instead_of_acknowledging() {
    let (respond, last) = new_order_daemon(0, true);
    let (relay, fake) = node_with(respond).await;
    let maker = Maker::connect(&relay, fake.pubkey()).await;

    let reply = only(
        maker
            .send(&request(legacy_order((100, None), 0, None), 1), WAIT)
            .await,
    );

    assert_eq!(reply.action, Action::PayBondInvoice);
    assert!(matches!(
        reply.payload,
        Some(Payload::PaymentRequest(Some(_), _, None))
    ));
    assert_eq!(*last.lock().unwrap(), 1, "the node consumed the index");
}

#[tokio::test]
async fn a_writable_seeded_store_gives_the_next_trade_index_from_the_recorded_one() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let _identity: Keys = seed_identity(&dir).await;
    let pool_of = || async {
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.join("mcli.db"));
        sqlx::SqlitePool::connect_with(options).await.unwrap()
    };
    let pool = pool_of().await;
    assert_eq!(
        mostro_client::db::User::get_next_trade_index(pool.clone())
            .await
            .unwrap(),
        1
    );
    pool.close().await;

    set_last_trade_index(&dir, 7).await;

    let pool = pool_of().await;
    assert_eq!(
        mostro_client::db::User::get_next_trade_index(pool.clone())
            .await
            .unwrap(),
        8
    );
    pool.close().await;
}

#[tokio::test]
async fn the_legacy_save_writes_the_order_row_and_the_trade_index_only_after_an_accepted_reply() {
    let sandbox = Sandbox::new().unwrap();
    let dir = sandbox.mcli_dir();
    let _identity: Keys = seed_identity(&dir).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.join("mcli.db"));
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    let rows = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM orders")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let (trade_keys, _) = mostro_client::db::User::get_next_trade_keys(&pool)
        .await
        .unwrap();
    assert_eq!(
        (
            rows().await,
            mostro_client::db::User::get_last_trade_index(pool.clone())
                .await
                .unwrap()
        ),
        (0, 0),
        "computing the next key writes nothing"
    );

    let mut accepted = legacy_order((100, None), 0, None);
    accepted.id = Some(uuid::Uuid::new_v4());
    mostro_client::util::save_order(accepted.clone(), &trade_keys, REQUEST_ID, 1, &pool)
        .await
        .unwrap();

    assert_eq!(rows().await, 1);
    assert_eq!(
        mostro_client::db::User::get_last_trade_index(pool.clone())
            .await
            .unwrap(),
        1
    );

    // The same order again replaces its row (no duplicate) and moves the index on.
    mostro_client::util::save_order(accepted.clone(), &trade_keys, REQUEST_ID, 2, &pool)
        .await
        .unwrap();
    assert_eq!(rows().await, 1);
    assert_eq!(
        mostro_client::db::User::get_last_trade_index(pool.clone())
            .await
            .unwrap(),
        2
    );
    pool.close().await;

    // A store that cannot be written fails the save and leaves the index behind
    // the node's: the next request would be refused as `invalid_trade_index`.
    let read_only = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(dir.join("mcli.db"))
        .read_only(true);
    let pool = sqlx::SqlitePool::connect_with(read_only).await.unwrap();
    let failed = mostro_client::util::save_order(accepted, &trade_keys, REQUEST_ID, 3, &pool).await;
    assert!(failed.is_err());
    assert_eq!(
        mostro_client::db::User::get_last_trade_index(pool.clone())
            .await
            .unwrap(),
        2
    );
    pool.close().await;
}

// --- The exchange with a trade key (spec 003, slice 2) ----------------------

fn limits() -> ExchangeLimits {
    ExchangeLimits {
        connect: Duration::from_secs(2),
        probe: Duration::from_millis(400),
        reply: Duration::from_millis(1_500),
        cleanup: Duration::from_millis(500),
        ..ExchangeLimits::default()
    }
}

fn expect_new() -> Expect {
    Expect {
        allowed_actions: vec![Action::NewOrder, Action::PayBondInvoice, Action::CantDo],
    }
}

async fn ask_node(
    respond: Respond,
    index: i64,
) -> (
    Result<mostro_client::msg::Exchanged, mostro_client::msg::ExchangeFailure>,
    FakeMostro,
    Keys,
    Keys,
    LoopbackRelay,
) {
    let (relay, fake) = node_with(respond).await;
    let (identity, trade) = (Keys::generate(), Keys::generate());
    let config = PublicReadConfig::new(fake.pubkey(), &relay.url).unwrap();
    let outbound = Outbound {
        identity: &identity,
        signer: &trade,
        recipient: fake.pubkey(),
        message: request(legacy_order((100, None), 0, None), index),
        pow: 0,
        check_protocol: true,
    };
    let result = exchange(&config, outbound, &expect_new(), &limits()).await;
    (result, fake, identity, trade, relay)
}

#[tokio::test]
async fn the_exchange_signs_with_the_trade_key_and_hears_the_reply_addressed_to_it() {
    let (respond, last) = new_order_daemon(0, false);

    let (result, fake, identity, trade, _relay) = ask_node(respond, 1).await;

    let exchanged = result.unwrap();
    let reply = exchanged.reply().get_inner_message_kind();
    assert_eq!(reply.action, Action::NewOrder);
    assert_eq!(reply.request_id, Some(REQUEST_ID));
    let seen = fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].author, trade.public_key());
    assert_eq!(seen[0].identity, identity.public_key());
    assert!(seen[0].signature_valid);
    assert_eq!(*last.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_bond_request_and_a_refusal_come_back_for_the_caller_to_judge() {
    let (respond, _) = new_order_daemon(0, true);
    let (result, ..) = ask_node(respond, 1).await;
    let exchanged = result.unwrap();
    assert_eq!(
        exchanged.reply().get_inner_message_kind().action,
        Action::PayBondInvoice
    );

    let (respond, last) = new_order_daemon(5, false);
    let (result, ..) = ask_node(respond, 3).await;
    let exchanged = result.unwrap();
    assert_eq!(
        exchanged.reply().get_inner_message_kind().action,
        Action::CantDo
    );
    assert_eq!(*last.lock().unwrap(), 5);
}

#[tokio::test]
async fn a_reply_addressed_to_the_identity_instead_of_the_trade_key_is_never_heard() {
    let respond: Respond = Arc::new(|request| {
        let ack = fake_mostro::order_created(request, uuid::Uuid::new_v4());
        vec![fake_mostro::Outgoing::to_requester(ack).addressed_to(request.identity)]
    });

    let (result, ..) = ask_node(respond, 1).await;

    let failed = result.unwrap_err();
    assert_eq!(
        failed.code(),
        mostro_client::outcome::ErrorCode::ReplyTimeout
    );
    assert_eq!(
        failed.publication(),
        mostro_client::outcome::Publication::Published
    );
}

// --- The store (spec 003, slice 2) ------------------------------------------

mod store {
    use super::*;
    use mostro_client::store::{Store, StoreError, Trade};

    async fn seeded() -> (Sandbox, Store, Keys) {
        let sandbox = Sandbox::new().unwrap();
        let identity = seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        (sandbox, store, identity)
    }

    fn directory(sandbox: &Sandbox) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(sandbox.mcli_dir())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read(entry.path()).unwrap_or_default(),
                )
            })
            .collect();
        files.sort();
        files
    }

    async fn scalar<T>(sandbox: &Sandbox, sql: &'static str) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let value = sqlx::query_scalar::<_, T>(sql)
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    fn created(id: uuid::Uuid) -> SmallOrder {
        let mut order = legacy_order((100, None), 0, None);
        order.id = Some(id);
        order.expires_at = Some(1_792_195_200);
        order
    }

    #[tokio::test]
    async fn the_next_trade_is_the_recorded_index_plus_one_and_reading_it_changes_nothing() {
        let (sandbox, store, _) = seeded().await;
        set_last_trade_index(&sandbox.mcli_dir(), 6).await;
        let before = directory(&sandbox);

        let trade = store.next_trade().await.unwrap();

        assert_eq!(trade.index, 7);
        assert_eq!(directory(&sandbox), before, "no file changed or appeared");
        assert!(!format!("{trade:?}").contains(&trade.keys.secret_key().to_secret_hex()));
        let again = store.next_trade().await.unwrap();
        assert_eq!(again.keys.public_key(), trade.keys.public_key());
    }

    #[tokio::test]
    async fn saving_writes_the_row_and_the_index_together_and_closes_cleanly() {
        let (sandbox, store, _) = seeded().await;
        let trade = store.next_trade().await.unwrap();
        let id = uuid::Uuid::new_v4();

        store
            .save_order(&created(id), &trade, REQUEST_ID)
            .await
            .unwrap();

        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT COUNT(*) FROM orders").await,
            1
        );
        assert_eq!(
            scalar::<String>(&sandbox, "SELECT id FROM orders").await,
            id.to_string()
        );
        assert_eq!(
            scalar::<String>(&sandbox, "SELECT trade_keys FROM orders").await,
            trade.keys.secret_key().to_secret_hex()
        );
        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT request_id FROM orders").await,
            REQUEST_ID as i64
        );
        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT expires_at FROM orders").await,
            1_792_195_200
        );
        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT last_trade_index FROM users").await,
            1
        );
        let names: Vec<_> = directory(&sandbox).into_iter().map(|f| f.0).collect();
        assert_eq!(names, vec!["mcli.db".to_string()], "no sidecar left behind");
        assert_eq!(store.next_trade().await.unwrap().index, 2);
    }

    #[tokio::test]
    async fn a_failed_save_writes_neither_the_row_nor_the_index() {
        let (sandbox, store, _) = seeded().await;
        let trade = store.next_trade().await.unwrap();
        // Make the index update fail after the row was inserted: no users table row.
        let options =
            sqlx::sqlite::SqliteConnectOptions::new().filename(sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DELETE FROM users")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let failed = store
            .save_order(&created(uuid::Uuid::new_v4()), &trade, REQUEST_ID)
            .await;

        assert_eq!(failed, Err(StoreError::Missing));
        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT COUNT(*) FROM orders").await,
            0
        );
    }

    #[tokio::test]
    async fn advance_moves_the_index_forward_only() {
        let (sandbox, store, _) = seeded().await;
        store.advance(4).await.unwrap();
        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT last_trade_index FROM users").await,
            4
        );
        store.advance(2).await.unwrap();
        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT last_trade_index FROM users").await,
            4,
            "an older index never moves it back"
        );
        store.advance(4).await.unwrap();
        assert_eq!(
            scalar::<i64>(&sandbox, "SELECT last_trade_index FROM users").await,
            4
        );
        assert_eq!(store.next_trade().await.unwrap().index, 5);
    }

    #[tokio::test]
    async fn a_missing_garbled_or_read_only_store_is_reported_and_nothing_is_created() {
        let sandbox = Sandbox::new().unwrap();
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = Trade {
            index: 1,
            keys: Keys::generate(),
        };
        assert_eq!(store.next_trade().await.unwrap_err(), StoreError::Missing);
        assert_eq!(store.advance(1).await, Err(StoreError::Missing));
        assert_eq!(
            store
                .save_order(&created(uuid::Uuid::new_v4()), &trade, 1)
                .await,
            Err(StoreError::Missing)
        );
        assert!(
            harness::is_empty_dir(&sandbox.mcli_dir()),
            "nothing was created"
        );

        std::fs::write(sandbox.mcli_dir().join("mcli.db"), b"not a database").unwrap();
        assert_eq!(
            store.next_trade().await.unwrap_err(),
            StoreError::Unwritable
        );
        assert_eq!(store.advance(1).await, Err(StoreError::Unwritable));
        assert_eq!(
            std::fs::read(sandbox.mcli_dir().join("mcli.db")).unwrap(),
            b"not a database"
        );

        let (sandbox, store, _) = seeded().await;
        let mut permissions = std::fs::metadata(sandbox.mcli_dir().join("mcli.db"))
            .unwrap()
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(sandbox.mcli_dir().join("mcli.db"), permissions).unwrap();
        let writable = std::fs::OpenOptions::new()
            .write(true)
            .open(sandbox.mcli_dir().join("mcli.db"))
            .is_ok();
        let checked = store.ensure_writable().await;
        assert_eq!(
            checked.is_ok(),
            writable,
            "the check says what a write would"
        );
        let blocked = store.advance(3).await;
        // A read-only file is reported unless the process may ignore the mode (root).
        if blocked.is_ok() {
            assert_eq!(
                scalar::<i64>(&sandbox, "SELECT last_trade_index FROM users").await,
                3
            );
        } else {
            assert_eq!(blocked, Err(StoreError::Unwritable));
        }
    }
}

// --- The operation (spec 003, slice 4a) -------------------------------------

mod operation {
    use super::*;
    use mostro_client::commands::order::new::{order_new, NewOptions, NewOutcome};
    use mostro_client::outcome::new::{OrderNewInput, OrderRequest};
    use mostro_client::outcome::{ErrorCode, Status};
    use mostro_client::store::Store;
    use serde_json::json;

    struct Maker {
        relay: LoopbackRelay,
        fake: FakeMostro,
        config: PublicReadConfig,
        identity: Keys,
        sandbox: Sandbox,
        store: Store,
    }

    async fn maker(respond: Respond, pow: u8) -> Maker {
        maker_with(respond, 2, pow, false).await
    }

    /// A node advertising `protocol` and `pow`; its relay refuses every write
    /// when `block_writes` is set.
    async fn maker_with(respond: Respond, protocol: u8, pow: u8, block_writes: bool) -> Maker {
        let node = Keys::generate();
        let relay = if block_writes {
            LocalRelay::builder()
                .write_policy(super::BlockWrites)
                .build()
        } else {
            LocalRelay::new()
        };
        relay
            .add_event(info_event(&node, Some(protocol), Some(pow), None, Some(10)))
            .await
            .unwrap();
        let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
            .await
            .unwrap();
        let fake = FakeMostro::start(&relay.url, node, respond).await;
        let config = PublicReadConfig::new(fake.pubkey(), &relay.url).unwrap();
        let sandbox = Sandbox::new().unwrap();
        let identity = seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        Maker {
            relay,
            fake,
            config,
            identity,
            sandbox,
            store,
        }
    }

    fn input() -> OrderNewInput {
        OrderNewInput::validate(OrderRequest {
            side: OrderKind::Sell,
            currency: "usd".into(),
            fiat_amount: 100,
            fiat_amount_max: None,
            payment_method: "bank transfer".into(),
            sats: 0,
            premium: 2,
            invoice: None,
            expiration_days: 0,
        })
        .unwrap()
    }

    async fn create(maker: &Maker) -> NewOutcome {
        let trade = maker.store.next_trade().await.unwrap();
        order_new(
            &input(),
            &maker.config,
            &maker.identity,
            &trade,
            &maker.store,
            NewOptions {
                pow: 0,
                check_protocol: true,
            },
            &super::limits(),
        )
        .await
    }

    async fn scalar<T>(maker: &Maker, sql: &'static str) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(maker.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let value = sqlx::query_scalar::<_, T>(sql)
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    async fn state(maker: &Maker) -> (i64, i64) {
        (
            scalar(maker, "SELECT COUNT(*) FROM orders").await,
            scalar(maker, "SELECT COALESCE(last_trade_index, 0) FROM users").await,
        )
    }

    fn wire(outcome: &NewOutcome) -> serde_json::Value {
        serde_json::to_value(outcome).unwrap()
    }

    fn stages(outcome: &NewOutcome) -> serde_json::Value {
        wire(outcome)["meta"].clone()
    }

    #[tokio::test]
    async fn an_accepted_order_is_returned_and_saved_with_its_index() {
        let (respond, last) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;

        let outcome = create(&maker).await;

        assert_eq!(outcome.status(), Status::Ok, "{:?}", wire(&outcome));
        let data = outcome.data().unwrap();
        assert_eq!(data.trade_index(), "1");
        let seen = maker.fake.received();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].trade_index, Some(1));
        assert!(seen[0].signature_valid);
        assert_eq!(seen[0].identity, maker.identity.public_key());
        assert_eq!(seen[0].author.to_hex(), data.trade_pubkey());
        assert_eq!(
            stages(&outcome),
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "applied", "snapshot": null})
        );
        assert_eq!(state(&maker).await, (1, 1));
        assert_eq!(*last.lock().unwrap(), 1);
        assert_eq!(
            scalar::<String>(&maker, "SELECT id FROM orders").await,
            data.order().id()
        );
        assert_eq!(maker.store.next_trade().await.unwrap().index, 2);
    }

    #[tokio::test]
    async fn a_refusal_is_an_error_with_its_reason_and_writes_nothing() {
        let (respond, _) = new_order_daemon(5, false);
        let maker = maker(respond, 0).await;

        let outcome = create(&maker).await;

        assert_eq!(outcome.status(), Status::Error);
        assert_eq!(outcome.error_code(), Some(ErrorCode::NodeRefused));
        assert_eq!(outcome.reason(), Some("invalid_trade_index"));
        assert_eq!(
            stages(&outcome),
            json!({"publication": "published", "daemon_acceptance": "rejected",
                   "persistence": "not_attempted", "snapshot": null})
        );
        assert_eq!(state(&maker).await, (0, 0), "a refusal spends no index");
    }

    #[tokio::test]
    async fn silence_and_an_odd_reply_are_uncertain_and_spend_the_index_without_an_order() {
        let silent = maker(super::fake_mostro::silent(), 0).await;
        let outcome = create(&silent).await;
        assert_eq!(outcome.status(), Status::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReplyTimeout));
        assert!(outcome.data().is_none());
        assert_eq!(
            stages(&outcome),
            json!({"publication": "published", "daemon_acceptance": "unknown",
                   "persistence": "applied", "snapshot": null})
        );
        assert_eq!(silent.fake.received().len(), 1, "never retried");
        assert_eq!(state(&silent).await, (0, 1));

        let odd: Respond = std::sync::Arc::new(|request| {
            // An allowed action with a payload that is not an order.
            let odd = Message::new_order(
                None,
                request.request_id,
                request.trade_index,
                Action::NewOrder,
                Some(Payload::TextMessage("not an order".to_string())),
            );
            vec![fake_mostro::Outgoing::to_requester(odd)]
        });
        let odd = maker(odd, 0).await;
        let outcome = create(&odd).await;
        assert_eq!(outcome.status(), Status::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReplyMalformed));
        assert_eq!(state(&odd).await, (0, 1));
        assert_eq!(odd.store.next_trade().await.unwrap().index, 2);
    }

    #[tokio::test]
    async fn a_bonded_node_is_an_error_that_spends_the_index() {
        let (respond, last) = new_order_daemon(0, true);
        let maker = maker(respond, 0).await;

        let outcome = create(&maker).await;

        assert_eq!(outcome.status(), Status::Error);
        assert_eq!(outcome.error_code(), Some(ErrorCode::BondRequired));
        assert_eq!(
            stages(&outcome),
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "applied", "snapshot": null})
        );
        assert_eq!(state(&maker).await, (0, 1));
        assert_eq!(*last.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn an_order_that_cannot_be_saved_is_uncertain_and_carries_what_the_node_made() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let trade = maker.store.next_trade().await.unwrap();
        // The node will accept; the store loses its user row meanwhile.
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(maker.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DELETE FROM users")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let outcome = order_new(
            &input(),
            &maker.config,
            &maker.identity,
            &trade,
            &maker.store,
            NewOptions {
                pow: 0,
                check_protocol: true,
            },
            &super::limits(),
        )
        .await;

        assert_eq!(outcome.status(), Status::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::OrderNotSaved));
        let data = outcome.data().expect("what the node created is returned");
        assert_eq!(data.trade_index(), "1");
        assert_eq!(
            stages(&outcome),
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "failed", "snapshot": null})
        );
        assert_eq!(
            scalar::<i64>(&maker, "SELECT COUNT(*) FROM orders").await,
            0
        );
    }

    #[tokio::test]
    async fn a_failed_order_save_still_spends_the_index_so_the_key_is_not_reused() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(maker.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DROP TABLE orders")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let outcome = create(&maker).await;

        assert_eq!(outcome.error_code(), Some(ErrorCode::OrderNotSaved));
        assert_eq!(
            scalar::<i64>(&maker, "SELECT last_trade_index FROM users").await,
            1,
            "the node consumed index 1, so the next order must use 2"
        );
        assert_eq!(maker.store.next_trade().await.unwrap().index, 2);
    }

    #[tokio::test]
    async fn work_the_node_expects_stops_the_command_before_anything_is_sent_or_written() {
        let (respond, last) = new_order_daemon(0, false);
        let maker = maker(respond, 9).await;
        let before = std::fs::read(maker.sandbox.mcli_dir().join("mcli.db")).unwrap();

        let outcome = create(&maker).await;

        assert_eq!(outcome.status(), Status::Error);
        assert_eq!(outcome.error_code(), Some(ErrorCode::PowRequired));
        assert_eq!(
            stages(&outcome),
            json!({"publication": "not_attempted", "daemon_acceptance": "unknown",
                   "persistence": "not_attempted", "snapshot": null})
        );
        assert!(maker.fake.received().is_empty());
        assert_eq!(*last.lock().unwrap(), 0);
        assert_eq!(state(&maker).await, (0, 0));
        assert_eq!(
            std::fs::read(maker.sandbox.mcli_dir().join("mcli.db")).unwrap(),
            before
        );
    }

    // --- The binary (slice 4b) ------------------------------------------------

    use harness::{run_with_watchdog, Captured, WATCHDOG};
    use std::process::Stdio;

    const BASE: [&str; 10] = [
        "order",
        "new",
        "--side",
        "sell",
        "--currency",
        "USD",
        "--fiat-amount",
        "100",
        "--payment-method",
        "bank transfer",
    ];

    fn args<'a>(front: &[&'a str], tail: &[&'a str]) -> Vec<&'a str> {
        front
            .iter()
            .chain(BASE.iter())
            .chain(tail)
            .copied()
            .collect()
    }

    fn command(maker: &Maker, args: &[&str]) -> tokio::process::Command {
        let mut command = maker.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", maker.config.mostro_pubkey().to_hex())
            .env("RELAYS", &maker.relay.url);
        command
    }

    async fn run(maker: &Maker, args: &[&str]) -> Captured {
        let run = run_with_watchdog(command(maker, args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    /// Run with `answer` as the whole of stdin: a file, so the child never
    /// waits on a pipe the test has to feed.
    async fn run_answering(maker: &Maker, args: &[&str], answer: &str) -> Captured {
        let path = std::env::temp_dir().join(format!(
            "mcli-answer-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, answer).unwrap();
        let mut command = command(maker, args);
        command.stdin(Stdio::from(std::fs::File::open(&path).unwrap()));
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    fn document(run: &Captured) -> serde_json::Value {
        let stdout = run.stdout_text();
        assert!(
            stdout.ends_with('\n') && stdout.lines().count() == 1,
            "{stdout}"
        );
        serde_json::from_str(&stdout).unwrap()
    }

    fn snapshot(maker: &Maker) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(maker.sandbox.mcli_dir())
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read(entry.path()).unwrap_or_default(),
                )
            })
            .collect();
        files.sort();
        files
    }

    fn conforms_to_the_checked_in_schema(outcome: &serde_json::Value) {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/schemas/order-new-v1.json"
        );
        let schema: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(
            schema_check::conforms(&schema, &schema, outcome),
            "{outcome}"
        );
    }

    #[tokio::test]
    async fn json_with_yes_creates_one_order_and_nothing_secret_is_shown() {
        let (respond, last) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;

        let run = run(&maker, &args(&["--output", "json"], &["--yes"])).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let outcome = document(&run);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["command"], "order.new");
        assert_eq!(outcome["status"], "ok");
        assert_eq!(outcome["data"]["trade_index"], "1");
        assert_eq!(outcome["data"]["order"]["fiat_currency"], "USD");
        assert_eq!(
            outcome["meta"],
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "applied", "snapshot": null})
        );
        assert_eq!(maker.fake.received().len(), 1);
        assert_eq!(state(&maker).await, (1, 1));
        assert_eq!(*last.lock().unwrap(), 1);
        assert_eq!(
            scalar::<String>(&maker, "SELECT id FROM orders").await,
            outcome["data"]["order"]["id"].as_str().unwrap()
        );
        // The trade private key and the mnemonic stay in the store.
        let secret = scalar::<String>(&maker, "SELECT trade_keys FROM orders").await;
        let mnemonic = scalar::<String>(&maker, "SELECT mnemonic FROM users").await;
        for stream in [run.stdout_text(), run.stderr_text()] {
            assert!(!stream.contains(&secret), "the trade key leaked");
            assert!(!stream.contains(&mnemonic), "the mnemonic leaked");
            assert!(!stream.contains("127.0.0.1"), "a relay address leaked");
        }
    }

    #[tokio::test]
    async fn json_without_yes_does_nothing_at_all() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let before = snapshot(&maker);

        let run = run(&maker, &args(&["--output", "json"], &[])).await;

        assert_eq!(run.code(), Some(2), "{}", run.stderr_text());
        assert!(run.stderr.is_empty());
        let outcome = document(&run);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["command"], "order.new");
        assert_eq!(outcome["error"]["code"], "invalid_arguments");
        assert!(maker.fake.received().is_empty());
        assert_eq!(snapshot(&maker), before);
    }

    #[tokio::test]
    async fn human_with_yes_prints_the_created_order_without_asking() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;

        let run = run(&maker, &args(&[], &["--yes"])).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        let out = run.stdout_text();
        assert!(out.contains("Order Created"), "{out}");
        assert!(!out.contains("Preview"), "no prompt, no preview: {out}");
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let id = scalar::<String>(&maker, "SELECT id FROM orders").await;
        assert!(out.contains(&id));
        assert_eq!(state(&maker).await, (1, 1));
    }

    #[tokio::test]
    async fn the_prompt_accepts_y_and_blank_and_declines_everything_else() {
        for answer in ["y\n", "\n", "Y\r\n"] {
            let (respond, _) = new_order_daemon(0, false);
            let maker = maker(respond, 0).await;
            let run = run_answering(&maker, &args(&[], &[]), answer).await;
            assert_eq!(run.code(), Some(0), "{answer:?}: {}", run.stderr_text());
            let out = run.stdout_text();
            assert!(out.contains("New Order Preview") && out.contains("Order Created"));
            assert!(run.stderr_text().contains("Create this order? [Y/n]"));
            assert_eq!(state(&maker).await, (1, 1), "{answer:?}");
        }
        for answer in ["n\n", "no\n", "maybe\n", ""] {
            let (respond, _) = new_order_daemon(0, false);
            let maker = maker(respond, 0).await;
            let before = snapshot(&maker);
            let run = run_answering(&maker, &args(&[], &[]), answer).await;
            assert_eq!(run.code(), Some(1), "{answer:?}: {}", run.stderr_text());
            let out = run.stdout_text();
            assert!(out.contains("New Order Preview"), "{out}");
            assert!(!out.contains("Order Created"), "{out}");
            assert!(run
                .stderr_text()
                .contains("error: The order was not confirmed, so nothing was sent."));
            assert!(maker.fake.received().is_empty(), "{answer:?}");
            assert_eq!(snapshot(&maker), before, "{answer:?}");
        }
        // A closed stdin is not approval either.
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let run = run(&maker, &args(&[], &[])).await;
        assert_eq!(run.code(), Some(1));
        assert!(maker.fake.received().is_empty());
    }

    #[tokio::test]
    async fn a_refusal_shows_its_reason_in_both_modes_and_spends_nothing() {
        let (respond, _) = new_order_daemon(5, false);
        let maker = maker(respond, 0).await;
        let before = snapshot(&maker);

        let json = run(&maker, &args(&["--output", "json"], &["--yes"])).await;
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["status"], "error");
        assert_eq!(outcome["error"]["code"], "node_refused");
        assert_eq!(outcome["error"]["reason"], "invalid_trade_index");
        assert_eq!(outcome["data"], serde_json::Value::Null);

        let human = run(&maker, &args(&[], &["--yes"])).await;
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout.is_empty());
        assert_eq!(
            human.stderr_text(),
            "error: The node refused the request. (reason: invalid_trade_index)\n"
        );
        assert_eq!(snapshot(&maker), before, "a refusal changes nothing");
        assert_eq!(maker.fake.received().len(), 2, "one request per run");
    }

    #[tokio::test]
    async fn a_silent_node_is_uncertain_after_the_production_budget_and_is_never_retried() {
        let maker = maker(super::fake_mostro::silent(), 0).await;
        let started = std::time::Instant::now();

        let run = run(&maker, &args(&["--output", "json"], &["--yes"])).await;

        assert!(started.elapsed() >= ExchangeLimits::default().reply);
        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        assert!(run.stderr.is_empty());
        let outcome = document(&run);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "reply_timeout");
        assert_eq!(
            outcome["meta"],
            json!({"publication": "published", "daemon_acceptance": "unknown",
                   "persistence": "applied", "snapshot": null})
        );
        assert_eq!(maker.fake.received().len(), 1, "one publication, no retry");
        assert_eq!(state(&maker).await, (0, 1));
    }

    #[tokio::test]
    async fn a_bonded_node_is_an_error_and_the_index_is_spent() {
        let (respond, _) = new_order_daemon(0, true);
        let maker = maker(respond, 0).await;

        let run = run(&maker, &args(&["--output", "json"], &["--yes"])).await;

        assert_eq!(run.code(), Some(1));
        let outcome = document(&run);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["status"], "error");
        assert_eq!(outcome["error"]["code"], "bond_required");
        assert_eq!(state(&maker).await, (0, 1));
    }

    #[tokio::test]
    async fn an_order_that_cannot_be_saved_is_uncertain_and_still_shows_what_was_made() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(maker.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DROP TABLE orders")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let json = run(&maker, &args(&["--output", "json"], &["--yes"])).await;
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema(&outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "order_not_saved");
        let id = outcome["data"]["order"]["id"].as_str().unwrap().to_string();
        assert_eq!(outcome["meta"]["persistence"], "failed");

        let human = run(&maker, &args(&[], &["--yes"])).await;
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout_text().contains("Order Created"));
        let err = human.stderr_text();
        assert!(
            err.contains("error: The node created the order but it could not be saved locally.")
        );
        assert!(
            err.contains("Check `order list` before trying again."),
            "{err}"
        );
        assert!(!human.stdout_text().contains(&id), "a new order, a new id");
        assert_eq!(
            scalar::<i64>(&maker, "SELECT last_trade_index FROM users").await,
            2,
            "each attempt spent its own index"
        );
    }

    #[tokio::test]
    async fn a_range_order_reaches_the_node_as_min_and_max_and_is_saved_as_one() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;

        let run = run(
            &maker,
            &args(
                &["--output", "json"],
                &["--fiat-amount-max", "300", "--yes"],
            ),
        )
        .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        let order = maker.fake.received()[0].order.clone().unwrap();
        assert_eq!(
            (order.fiat_amount, order.min_amount, order.max_amount),
            (0, Some(100), Some(300))
        );
        let outcome = document(&run);
        assert_eq!(outcome["data"]["order"]["fiat_amount_min"], "100");
        assert_eq!(outcome["data"]["order"]["fiat_amount_max"], "300");
        assert_eq!(
            scalar::<i64>(&maker, "SELECT max_amount FROM orders").await,
            300
        );
    }

    #[tokio::test]
    async fn verbose_adds_one_code_only_line() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let mut flags = vec!["--verbose"];
        flags.extend(["--output", "json"]);

        let run = run(&maker, &args(&flags, &["--yes"])).await;

        assert_eq!(run.code(), Some(0));
        assert_eq!(run.stderr_text(), "verbose: order.new ok\n");
    }

    // --- The failure matrix (slice 5) -----------------------------------------

    const SENTINEL: &str = "SENTINEL-zq81";

    /// How the store of the sandbox is damaged before the run.
    #[derive(Clone, Copy, Debug)]
    enum Tweak {
        None,
        Absent,
        Garbage,
        EmptyUsers,
        NoUsersTable,
        NullMnemonic,
        ReadOnly,
    }

    async fn tweak(maker: &Maker, how: Tweak) {
        let path = maker.sandbox.mcli_dir().join("mcli.db");
        let sql = |statement: &'static str| {
            let path = path.clone();
            async move {
                let options = sqlx::sqlite::SqliteConnectOptions::new().filename(path);
                let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
                sqlx::query(statement).execute(&pool).await.unwrap();
                pool.close().await;
            }
        };
        match how {
            Tweak::None => {}
            Tweak::Absent => std::fs::remove_file(&path).unwrap(),
            Tweak::Garbage => std::fs::write(&path, b"this is not a sqlite database").unwrap(),
            Tweak::EmptyUsers => sql("DELETE FROM users").await,
            Tweak::NoUsersTable => sql("DROP TABLE users").await,
            Tweak::NullMnemonic => sql("UPDATE users SET mnemonic = NULL").await,
            Tweak::ReadOnly => {
                let mut permissions = std::fs::metadata(&path).unwrap().permissions();
                permissions.set_readonly(true);
                std::fs::set_permissions(&path, permissions).unwrap();
            }
        }
    }

    /// Whether the process can still write a file marked read-only (root).
    fn ignores_read_only(maker: &Maker) -> bool {
        std::fs::OpenOptions::new()
            .write(true)
            .open(maker.sandbox.mcli_dir().join("mcli.db"))
            .is_ok()
    }

    struct Failing {
        label: &'static str,
        tweak: Tweak,
        set: &'static [(&'static str, &'static str)],
        unset: &'static [&'static str],
        /// Global flags placed before the command.
        flags: &'static [&'static str],
        code: &'static str,
    }

    impl Failing {
        const fn new(label: &'static str, tweak: Tweak, code: &'static str) -> Self {
            Self {
                label,
                tweak,
                set: &[],
                unset: &[],
                flags: &[],
                code,
            }
        }
    }

    async fn run_env(
        maker: &Maker,
        args: &[&str],
        set: &[(&str, &str)],
        unset: &[&str],
    ) -> Captured {
        let mut command = command(maker, args);
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

    /// Run `case` as JSON, human and verbose and check what every mode shows,
    /// then that nothing was sent and nothing changed.
    async fn check_failure(case: &Failing, maker: &Maker) {
        tweak(maker, case.tweak).await;
        let before = snapshot(maker);
        let label = case.label;
        for mode in [&["--output", "json"][..], &[][..], &["--verbose"][..]] {
            let mut front: Vec<&str> = case.flags.to_vec();
            front.extend(mode);
            let run = run_env(maker, &args(&front, &["--yes"]), case.set, case.unset).await;
            assert_eq!(
                run.code(),
                Some(1),
                "{label} {mode:?}: {}",
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
            if mode == ["--output", "json"] {
                let outcome = document(&run);
                conforms_to_the_checked_in_schema(&outcome);
                assert_eq!(outcome["command"], "order.new", "{label}");
                assert_eq!(outcome["status"], "error", "{label}");
                assert_eq!(outcome["data"], serde_json::Value::Null, "{label}");
                assert_eq!(outcome["error"]["code"], case.code, "{label}");
                assert_eq!(
                    outcome["meta"],
                    json!({"publication": "not_attempted", "daemon_acceptance": "unknown",
                           "persistence": "not_attempted", "snapshot": null}),
                    "{label}"
                );
                assert!(stderr.is_empty(), "{label}: {stderr}");
            } else {
                assert!(stdout.is_empty(), "{label} {mode:?}: {stdout}");
                assert!(stderr.starts_with("error: "), "{label}: {stderr}");
                assert!(
                    !stderr.contains("Check `order list`"),
                    "{label}: not uncertain"
                );
            }
        }
        assert!(
            maker.fake.received().is_empty(),
            "{label}: something was sent"
        );
        if !matches!(case.tweak, Tweak::ReadOnly) {
            assert_eq!(snapshot(maker), before, "{label}: the store changed");
        }
        assert!(
            harness::is_empty_dir(&maker.sandbox.home())
                && harness::is_empty_dir(&maker.sandbox.cwd()),
            "{label}: files were created elsewhere"
        );
    }

    #[tokio::test]
    async fn failures_before_any_request_are_stable_and_leave_no_trace() {
        let cases = [
            Failing::new("no store", Tweak::Absent, "identity_missing"),
            Failing::new("no user", Tweak::EmptyUsers, "identity_missing"),
            Failing::new("not a database", Tweak::Garbage, "identity_unreadable"),
            Failing::new("no users table", Tweak::NoUsersTable, "identity_unreadable"),
            Failing::new("null mnemonic", Tweak::NullMnemonic, "identity_unreadable"),
            Failing {
                unset: &["MOSTRO_PUBKEY"],
                ..Failing::new("no node key", Tweak::None, "configuration_missing")
            },
            Failing {
                unset: &["RELAYS"],
                ..Failing::new("no relays", Tweak::None, "configuration_missing")
            },
            Failing {
                set: &[("MOSTRO_PUBKEY", SENTINEL)],
                ..Failing::new("bad node key", Tweak::None, "configuration_invalid")
            },
            Failing {
                set: &[("RELAYS", "http://SENTINEL-zq81.invalid")],
                ..Failing::new("bad relay", Tweak::None, "configuration_invalid")
            },
            Failing {
                flags: &["--pow", SENTINEL],
                ..Failing::new("bad pow", Tweak::None, "configuration_invalid")
            },
            Failing {
                flags: &["--transport", SENTINEL],
                ..Failing::new("bad transport", Tweak::None, "configuration_invalid")
            },
        ];
        for case in &cases {
            let (respond, last) = new_order_daemon(0, false);
            let maker = maker(respond, 0).await;
            check_failure(case, &maker).await;
            assert_eq!(*last.lock().unwrap(), 0, "{}", case.label);
        }
    }

    #[tokio::test]
    async fn a_store_that_cannot_be_written_stops_the_command_before_anything_is_sent() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        tweak(&maker, Tweak::ReadOnly).await;
        if ignores_read_only(&maker) {
            return; // running as a user the file mode does not bind
        }
        let case = Failing::new("read-only store", Tweak::None, "store_unwritable");
        check_failure(&case, &maker).await;
    }

    #[tokio::test]
    async fn a_node_that_cannot_be_served_fails_before_publishing() {
        let (respond, _) = new_order_daemon(0, false);
        let old = maker_with(respond, 1, 0, false).await;
        check_failure(
            &Failing::new("protocol 1", Tweak::None, "protocol_unsupported"),
            &old,
        )
        .await;

        let (respond, _) = new_order_daemon(0, false);
        let work = maker_with(respond, 2, 9, false).await;
        check_failure(
            &Failing::new("more work expected", Tweak::None, "pow_required"),
            &work,
        )
        .await;

        // An explicit transport skips the protocol check and enough work passes.
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker_with(respond, 1, 9, false).await;
        let run = run(
            &maker,
            &args(
                &["--transport", "nip44", "--pow", "9", "--output", "json"],
                &["--yes"],
            ),
        )
        .await;
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    }

    #[tokio::test]
    async fn a_relay_that_rejects_the_request_leaves_the_index_unspent() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker_with(respond, 2, 0, true).await;
        check_failure(
            &Failing::new("rejected", Tweak::None, "request_rejected"),
            &maker,
        )
        .await;
        assert_eq!(state(&maker).await, (0, 0));
    }

    #[tokio::test]
    async fn an_unreachable_relay_is_a_connection_failure_and_costs_nothing() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("ws://{}", free.local_addr().unwrap());
        drop(free);
        let before = snapshot(&maker);

        let run = run_env(
            &maker,
            &args(&["--output", "json"], &["--yes"]),
            &[("RELAYS", dead.as_str())],
            &[],
        )
        .await;

        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        let outcome = document(&run);
        assert_eq!(outcome["error"]["code"], "relay_connection_failed");
        assert_eq!(outcome["status"], "error");
        assert_eq!(outcome["meta"]["publication"], "not_attempted");
        assert!(!run.stderr_text().contains("127.0.0.1"));
        assert_eq!(snapshot(&maker), before);
    }

    #[tokio::test]
    async fn invalid_input_is_an_argument_error_before_any_effect() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let before = snapshot(&maker);
        let swap = |option: &'static str, value: &'static str| -> Vec<&'static str> {
            let mut all = vec!["--output", "json"];
            let mut base = BASE.to_vec();
            let at = base.iter().position(|a| *a == option).unwrap();
            base[at + 1] = value;
            all.extend(base);
            all.push("--yes");
            all
        };
        let extra =
            |tail: &[&'static str]| -> Vec<&'static str> { args(&["--output", "json"], tail) };
        let bell = "SENTINEL-zq81\u{7}";
        let cases: Vec<(Vec<&str>, serde_json::Value)> = vec![
            (swap("--currency", "U$D"), json!("order.new")),
            (swap("--fiat-amount", "0"), json!("order.new")),
            (swap("--payment-method", bell), json!("order.new")),
            (
                extra(&["--fiat-amount-max", "50", "--yes"]),
                json!("order.new"),
            ),
            (
                extra(&["--fiat-amount-max", "300", "--sats", "1000", "--yes"]),
                json!("order.new"),
            ),
            (extra(&["--premium", "101", "--yes"]), json!("order.new")),
            (
                extra(&["--expiration-days", "366", "--yes"]),
                json!("order.new"),
            ),
            (
                extra(&["--invoice", "SENTINEL zq", "--yes"]),
                json!("order.new"),
            ),
            // Grammar errors keep the resolved command; a removed spelling has none.
            (swap("--side", "sideways"), json!("order.new")),
            (
                extra(&["--sats", "SENTINEL-zq81", "--yes"]),
                json!("order.new"),
            ),
            (
                vec!["--output", "json", "order", "new", "--yes"],
                json!("order.new"),
            ),
            (
                vec!["--output", "json", "neworder", "-k", "sell", "--yes"],
                serde_json::Value::Null,
            ),
        ];
        for (arguments, command) in cases {
            let run = run(&maker, &arguments).await;
            assert_eq!(run.code(), Some(2), "{arguments:?}: {}", run.stderr_text());
            let outcome = document(&run);
            assert_eq!(outcome["command"], command, "{arguments:?}");
            assert_eq!(
                outcome["error"]["code"], "invalid_arguments",
                "{arguments:?}"
            );
            assert_eq!(
                outcome["meta"]["publication"], "not_attempted",
                "{arguments:?}"
            );
            assert!(!run.stdout_text().contains(SENTINEL), "{arguments:?}");
            assert!(
                run.stderr.is_empty(),
                "{arguments:?}: {}",
                run.stderr_text()
            );
        }
        assert!(maker.fake.received().is_empty());
        assert_eq!(snapshot(&maker), before);
    }

    #[tokio::test]
    async fn an_unwritable_stdout_after_the_order_was_made_exits_1_with_one_fixed_line() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        for (front, line) in [
            (
                &["--output", "json"][..],
                "error: the JSON outcome could not be written\n",
            ),
            (&[][..], "error: the new order could not be written\n"),
        ] {
            let mut command = command(&maker, &args(front, &["--yes"]));
            command.stdout(harness::closed_pipe());
            let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
            assert_eq!(run.code(), Some(1), "{front:?}: {}", run.stderr_text());
            assert_eq!(run.stderr_text(), line, "{front:?}");
        }
        // The orders were created and saved all the same: each run published one.
        assert_eq!(state(&maker).await, (2, 2));
        assert_eq!(maker.fake.received().len(), 2);
    }

    #[tokio::test]
    async fn an_unwritable_stderr_changes_neither_the_exit_nor_the_document() {
        let (respond, _) = new_order_daemon(0, false);
        let maker = maker(respond, 0).await;
        let mut command = command(
            &maker,
            &args(&["--output", "json", "--pow", "x"], &["--yes"]),
        );
        command.stderr(harness::closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1));
        assert_eq!(document(&run)["error"]["code"], "configuration_invalid");
    }
}
