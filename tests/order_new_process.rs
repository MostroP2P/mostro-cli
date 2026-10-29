//! Tests for `order new` (spec 003).
//!
//! Slice 1 holds the characterization of what the legacy `neworder` puts on the
//! wire and the self-tests of the fixtures built for the command: the scripted
//! daemon that applies the source's `check_trade_index` and `order_action`
//! rules, and the writable identity store. The command's own scenarios are
//! added with the slices that implement it.

use std::time::Duration;

use futures::StreamExt;
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, Payload, Transport,
    WrapOptions,
};
use nostr_sdk::prelude::{
    Client, ClientNotification, Event, Filter, Keys, Kind, LocalRelay, PublicKey,
};
use serde_json::Value;
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
