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
    Client, ClientNotification, Event, Filter, Keys, Kind, LocalRelay, PublicKey,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::time::timeout;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{
    info_event, new_order_daemon, seed_identity, set_last_trade_index, FakeMostro, Respond,
};
use harness::{Backend, LoopbackRelay, Sandbox};

const REQUEST_ID: u64 = 4410;

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
        _relay: LoopbackRelay,
        fake: FakeMostro,
        config: PublicReadConfig,
        identity: Keys,
        sandbox: Sandbox,
        store: Store,
    }

    async fn maker(respond: Respond, pow: u8) -> Maker {
        let node = Keys::generate();
        let relay = LocalRelay::new();
        relay
            .add_event(info_event(&node, Some(2), Some(pow), None, Some(10)))
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
            _relay: relay,
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
}
