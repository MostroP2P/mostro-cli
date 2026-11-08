//! Tests for `order add-invoice` and `bond add-invoice` (spec 005).
//!
//! Slice 1 holds the self-tests of the fixtures built for the commands: the
//! scripted daemon that applies the source's `add_invoice_action` and
//! `add_bond_invoice_action` rules. The commands' own scenarios are added with
//! the slices that implement them.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use mostro_core::order::Status;
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, MessageKind, Payload,
    Transport, WrapOptions,
};
use nostr_sdk::prelude::{Client, ClientNotification, Filter, Keys, Kind, LocalRelay, PublicKey};
use tokio::time::timeout;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{
    info_event, invoice_daemon, payout_invoice, seed_identity, BondOwed, FakeMostro, Ledger, Owed,
    Respond,
};
use harness::{Backend, LoopbackRelay, Sandbox};
use mostro_client::store::{Store, StoreError, Trade};

/// Rejects every write with a machine-readable refusal.
#[derive(Debug)]
struct BlockWrites;

impl nostr_sdk::prelude::WritePolicy for BlockWrites {
    fn admit_event<'a>(
        &'a self,
        _event: &'a nostr_sdk::prelude::Event,
        _addr: &'a std::net::SocketAddr,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = nostr_sdk::prelude::WritePolicyResult> + Send + 'a>,
    > {
        Box::pin(async {
            nostr_sdk::prelude::WritePolicyResult::reject(
                nostr_sdk::prelude::MachineReadablePrefix::Blocked,
                "writes are off",
            )
        })
    }
}

const REQUEST_ID: u64 = 7150;
const WAIT: Duration = Duration::from_millis(700);
const INVOICE: &str = "lnbcrt1scriptedbuyerinvoice";

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// A client listening on one key and able to send as any identity and trade key.
struct Peer {
    client: Client,
    node: PublicKey,
}

impl Peer {
    async fn connect(relay: &LoopbackRelay, node: PublicKey) -> Self {
        let client = Client::default();
        client.add_relay(relay.url.as_str()).await.unwrap();
        let connected = client.try_connect().timeout(Duration::from_secs(5)).await;
        assert!(connected.failed.is_empty(), "{:?}", connected.failed);
        Self { client, node }
    }

    /// Listen on every key in `keys`, run `act`, and return what the node sent
    /// to each of them meanwhile, in the order of `keys`.
    async fn hear_all(
        &self,
        keys: &[&Keys],
        act: impl std::future::Future<Output = ()>,
    ) -> Vec<Vec<Message>> {
        let mut notifications = self.client.notifications();
        for key in keys {
            let filter = Filter::new()
                .kind(Kind::PrivateDirectMessage)
                .pubkey(key.public_key())
                .author(self.node)
                .limit(0);
            self.client.subscribe(filter).await.unwrap();
        }
        act.await;
        let mut heard = vec![Vec::new(); keys.len()];
        let _ = timeout(WAIT, async {
            while let Some(notification) = notifications.next().await {
                let ClientNotification::Event { event, .. } = notification else {
                    continue;
                };
                for (index, key) in keys.iter().enumerate() {
                    if let Ok(Some(seen)) = unwrap_incoming(&event, key).await {
                        heard[index].push(seen.message);
                    }
                }
            }
        })
        .await;
        heard
    }

    async fn hear(&self, key: &Keys, act: impl std::future::Future<Output = ()>) -> Vec<Message> {
        self.hear_all(&[key], act).await.remove(0)
    }

    async fn send(&self, message: &Message, identity: &Keys, trade: &Keys) {
        let event = wrap_message_with(
            Transport::Nip44Direct,
            message,
            identity,
            trade,
            self.node,
            WrapOptions::default(),
        )
        .await
        .unwrap();
        self.client.send_event(&event).await.unwrap();
    }
}

fn add_invoice(order: Uuid, invoice: Option<&str>) -> Message {
    Message::new_order(
        Some(order),
        Some(REQUEST_ID),
        None,
        Action::AddInvoice,
        invoice.map(|invoice| Payload::PaymentRequest(None, invoice.to_string(), None)),
    )
}

fn add_bond_invoice(order: Uuid, invoice: &str) -> Message {
    Message::new_order(
        Some(order),
        Some(REQUEST_ID),
        None,
        Action::AddBondInvoice,
        Some(Payload::PaymentRequest(None, invoice.to_string(), None)),
    )
}

struct World {
    _relay: LoopbackRelay,
    fake: FakeMostro,
    peer: Peer,
    ledger: Arc<Mutex<Ledger>>,
    buyer: Keys,
}

async fn world(orders: impl FnOnce(PublicKey) -> Vec<Owed>, acks: bool, share: i64) -> World {
    let buyer = Keys::generate();
    let ledger = Ledger {
        orders: orders(buyer.public_key()),
        bonds: vec![BondOwed {
            order: id(9),
            recipient: buyer.public_key(),
            share,
            open: true,
            invoice: None,
        }],
    };
    let (respond, ledger): (Respond, _) = invoice_daemon(ledger, acks);
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
    let peer = Peer::connect(&relay, fake.pubkey()).await;
    World {
        _relay: relay,
        fake,
        peer,
        ledger,
        buyer,
    }
}

impl World {
    /// Answer as `trade`; the replies it hears.
    async fn answer_as(&self, trade: &Keys, message: Message) -> Vec<Message> {
        let identity = Keys::generate();
        self.peer
            .hear(trade, self.peer.send(&message, &identity, trade))
            .await
    }

    async fn answer(&self, message: Message) -> Vec<Message> {
        self.answer_as(&self.buyer, message).await
    }
}

fn only(replies: Vec<Message>) -> MessageKind {
    assert_eq!(replies.len(), 1, "{replies:?}");
    replies[0].get_inner_message_kind().clone()
}

fn reason(kind: &MessageKind) -> CantDoReason {
    assert_eq!(kind.action, Action::CantDo);
    match &kind.payload {
        Some(Payload::CantDo(Some(reason))) => reason.clone(),
        other => panic!("a refusal, got {other:?}"),
    }
}

fn waiting(buyer: PublicKey) -> Vec<Owed> {
    vec![Owed::waiting(id(1), buyer)]
}

#[tokio::test]
async fn an_invoice_for_a_waiting_order_is_answered_with_waiting_seller_to_pay() {
    let world = world(waiting, true, 5_000).await;

    let reply = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);

    assert_eq!(reply.action, Action::WaitingSellerToPay);
    assert!(reply.payload.is_none(), "no payload, as in the source");
    assert_eq!(reply.id, Some(id(1)), "the message id is the order id");
    assert_eq!(
        reply.request_id,
        Some(REQUEST_ID),
        "the request id is echoed"
    );
    let ledger = world.ledger.lock().unwrap();
    assert_eq!(ledger.orders[0].status, Status::WaitingPayment);
    assert_eq!(ledger.orders[0].invoice.as_deref(), Some(INVOICE));
    let seen = world.fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].trade_index, None,
        "an invoice carries no trade index"
    );
    assert_eq!(seen[0].invoice.as_deref(), Some(INVOICE));
    assert_eq!(seen[0].author, world.buyer.public_key());
}

#[tokio::test]
async fn an_order_whose_seller_already_paid_goes_active_with_the_order_in_the_reply() {
    let world = world(
        |buyer| {
            vec![Owed {
                preimage: true,
                ..Owed::waiting(id(1), buyer)
            }]
        },
        true,
        5_000,
    )
    .await;

    let reply = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);

    assert_eq!(reply.action, Action::HoldInvoicePaymentAccepted);
    let Some(Payload::Order(order)) = reply.payload else {
        panic!("an order payload");
    };
    assert_eq!(
        (order.id, order.status),
        (Some(id(1)), Some(Status::Active))
    );
    assert_eq!(
        world.ledger.lock().unwrap().orders[0].status,
        Status::Active
    );
}

#[tokio::test]
async fn a_replacement_invoice_on_a_settled_order_is_updated_without_a_payload() {
    let world = world(
        |buyer| {
            vec![Owed {
                status: Status::SettledHoldInvoice,
                ..Owed::waiting(id(1), buyer)
            }]
        },
        true,
        5_000,
    )
    .await;

    let reply = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);

    assert_eq!(reply.action, Action::InvoiceUpdated);
    assert!(reply.payload.is_none());
    assert_eq!(reply.request_id, Some(REQUEST_ID));
    let ledger = world.ledger.lock().unwrap();
    assert_eq!(ledger.orders[0].status, Status::SettledHoldInvoice);
    assert_eq!(ledger.orders[0].invoice.as_deref(), Some(INVOICE));
}

#[tokio::test]
async fn every_add_invoice_refusal_leaves_the_order_untouched() {
    let stranger = Keys::generate();
    let cases: Vec<(&str, Message, bool, CantDoReason)> = vec![
        (
            "an unknown order",
            add_invoice(id(2), Some(INVOICE)),
            false,
            CantDoReason::NotFound,
        ),
        (
            "another key",
            add_invoice(id(1), Some(INVOICE)),
            true,
            CantDoReason::InvalidPeer,
        ),
        (
            "no invoice",
            add_invoice(id(1), None),
            false,
            CantDoReason::InvalidInvoice,
        ),
        (
            "a bad invoice",
            add_invoice(id(1), Some("not an invoice")),
            false,
            CantDoReason::InvalidInvoice,
        ),
    ];
    for (name, message, as_stranger, expected) in cases {
        let world = world(waiting, true, 5_000).await;
        let replies = if as_stranger {
            world.answer_as(&stranger, message).await
        } else {
            world.answer(message).await
        };
        assert_eq!(reason(&only(replies)), expected, "{name}");
        let ledger = world.ledger.lock().unwrap();
        assert_eq!(
            ledger.orders[0].status,
            Status::WaitingBuyerInvoice,
            "{name}"
        );
        assert_eq!(ledger.orders[0].invoice, None, "{name}");
    }
    let world = world(
        |buyer| {
            vec![Owed {
                status: Status::Active,
                ..Owed::waiting(id(1), buyer)
            }]
        },
        true,
        5_000,
    )
    .await;
    let kind = only(world.answer(add_invoice(id(1), Some(INVOICE))).await);
    assert_eq!(reason(&kind), CantDoReason::NotAllowedByStatus);
    assert_eq!(world.ledger.lock().unwrap().orders[0].invoice, None);
}

#[tokio::test]
async fn an_accepted_bond_invoice_is_acknowledged_without_a_request_id() {
    let world = world(waiting, true, 5_000).await;

    let reply = only(
        world
            .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
            .await,
    );

    assert_eq!(reply.action, Action::BondInvoiceAccepted);
    assert_eq!(
        reply.request_id, None,
        "the source sets no request id on this ack"
    );
    assert_eq!(reply.id, Some(id(9)));
    let Some(Payload::Order(order)) = reply.payload else {
        panic!("an order payload");
    };
    assert_eq!(order.amount, 5_000, "the counterparty share");
    let ledger = world.ledger.lock().unwrap();
    assert!(!ledger.bonds[0].open);
    assert_eq!(ledger.bonds[0].invoice, Some(payout_invoice(5_000)));
    assert_eq!(world.fake.received()[0].trade_index, None);
}

#[tokio::test]
async fn an_older_node_accepts_a_bond_invoice_in_silence() {
    let world = world(waiting, false, 5_000).await;

    let replies = world
        .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
        .await;

    assert!(replies.is_empty(), "{replies:?}");
    assert!(
        !world.ledger.lock().unwrap().bonds[0].open,
        "but it was accepted"
    );
}

#[tokio::test]
async fn every_bond_invoice_refusal_leaves_the_bond_open() {
    let stranger = Keys::generate();
    let cases: Vec<(&str, Message, bool, CantDoReason)> = vec![
        (
            "a wrong amount",
            add_bond_invoice(id(9), &payout_invoice(4_999)),
            false,
            CantDoReason::InvalidInvoice,
        ),
        (
            "a Lightning address",
            add_bond_invoice(id(9), "buyer@example.com"),
            false,
            CantDoReason::InvalidInvoice,
        ),
        (
            "an order with no bond",
            add_bond_invoice(id(1), &payout_invoice(5_000)),
            false,
            CantDoReason::NotAllowedByStatus,
        ),
        (
            "another key",
            add_bond_invoice(id(9), &payout_invoice(5_000)),
            true,
            CantDoReason::NotAllowedByStatus,
        ),
    ];
    for (name, message, as_stranger, expected) in cases {
        let world = world(waiting, true, 5_000).await;
        let replies = if as_stranger {
            world.answer_as(&stranger, message).await
        } else {
            world.answer(message).await
        };
        assert_eq!(reason(&only(replies)), expected, "{name}");
        let ledger = world.ledger.lock().unwrap();
        assert!(ledger.bonds[0].open, "{name}");
        assert_eq!(ledger.bonds[0].invoice, None, "{name}");
    }
}

#[tokio::test]
async fn a_second_bond_invoice_for_a_paid_out_bond_is_refused() {
    let world = world(waiting, true, 5_000).await;
    let first = world
        .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
        .await;
    assert_eq!(only(first).action, Action::BondInvoiceAccepted);

    let second = world
        .answer(add_bond_invoice(id(9), &payout_invoice(5_000)))
        .await;

    assert_eq!(reason(&only(second)), CantDoReason::NotAllowedByStatus);
}

#[tokio::test]
async fn any_other_action_is_refused() {
    let world = world(waiting, true, 5_000).await;
    let message = Message::new_order(Some(id(1)), Some(REQUEST_ID), None, Action::Release, None);

    let kind = only(world.answer(message).await);

    assert_eq!(reason(&kind), CantDoReason::InvalidAction);
}

// --- The store (spec 005, slice 2) ------------------------------------------

mod store {
    use super::*;

    async fn stored() -> (Sandbox, Store, Trade) {
        let sandbox = Sandbox::new().unwrap();
        seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = store.next_trade().await.unwrap();
        let order =
            Owed::waiting(id(1), trade.keys.public_key()).small(Status::WaitingBuyerInvoice);
        store.save_order(&order, &trade, REQUEST_ID).await.unwrap();
        (sandbox, store, trade)
    }

    async fn pool(sandbox: &Sandbox) -> sqlx::SqlitePool {
        let options =
            sqlx::sqlite::SqliteConnectOptions::new().filename(sandbox.mcli_dir().join("mcli.db"));
        sqlx::SqlitePool::connect_with(options).await.unwrap()
    }

    async fn status(sandbox: &Sandbox, order: Uuid) -> String {
        let pool = pool(sandbox).await;
        let value = sqlx::query_scalar("SELECT status FROM orders WHERE id = ?1")
            .bind(order.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    async fn index(sandbox: &Sandbox) -> i64 {
        let pool = pool(sandbox).await;
        let value = sqlx::query_scalar("SELECT last_trade_index FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    async fn execute(sandbox: &Sandbox, sql: &'static str) {
        let pool = pool(sandbox).await;
        sqlx::query(sql).execute(&pool).await.unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn the_key_of_a_saved_order_comes_back_and_nothing_else_does() {
        let (_sandbox, store, trade) = stored().await;

        let keys = store.order_keys(id(1)).await.unwrap().expect("the key");

        assert_eq!(keys.public_key(), trade.keys.public_key());
        assert!(store.order_keys(id(2)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_row_without_a_usable_key_is_the_same_as_no_order() {
        let (sandbox, store, _) = stored().await;

        execute(&sandbox, "UPDATE orders SET trade_keys = NULL").await;
        assert!(store.order_keys(id(1)).await.unwrap().is_none(), "null");
        execute(&sandbox, "UPDATE orders SET trade_keys = 'not a key'").await;
        assert!(store.order_keys(id(1)).await.unwrap().is_none(), "garbage");
    }

    #[tokio::test]
    async fn a_missing_store_is_missing_not_unknown() {
        let sandbox = Sandbox::new().unwrap();
        let store = Store::in_dir(&sandbox.mcli_dir());

        assert_eq!(
            store.order_keys(id(1)).await.unwrap_err(),
            StoreError::Missing
        );
        assert!(
            !sandbox.mcli_dir().join("mcli.db").exists(),
            "never created"
        );
    }

    #[tokio::test]
    async fn reading_a_key_changes_no_byte() {
        let (sandbox, store, _) = stored().await;
        let before = std::fs::read(sandbox.mcli_dir().join("mcli.db")).unwrap();

        store.order_keys(id(1)).await.unwrap();
        store.order_keys(id(2)).await.unwrap();

        assert_eq!(
            std::fs::read(sandbox.mcli_dir().join("mcli.db")).unwrap(),
            before
        );
    }

    #[tokio::test]
    async fn a_status_update_changes_that_order_and_never_the_trade_index() {
        let (sandbox, store, trade) = stored().await;
        let second =
            Owed::waiting(id(2), trade.keys.public_key()).small(Status::WaitingBuyerInvoice);
        let other = Trade {
            index: trade.index + 1,
            keys: Keys::generate(),
        };
        store.save_order(&second, &other, REQUEST_ID).await.unwrap();
        let before_index = index(&sandbox).await;

        store
            .set_status(id(1), Status::WaitingPayment)
            .await
            .unwrap();

        let first = status(&sandbox, id(1)).await;
        let untouched = status(&sandbox, id(2)).await;
        assert_eq!(
            (first.as_str(), untouched.as_str()),
            ("waiting-payment", "waiting-buyer-invoice")
        );
        assert_eq!(index(&sandbox).await, before_index);
    }

    #[tokio::test]
    async fn a_status_update_for_an_unknown_order_fails_and_writes_nothing() {
        let (sandbox, store, _) = stored().await;
        let before = std::fs::read(sandbox.mcli_dir().join("mcli.db")).unwrap();

        let error = store.set_status(id(3), Status::Active).await.unwrap_err();

        assert_eq!(error, StoreError::Unwritable);
        assert_eq!(
            std::fs::read(sandbox.mcli_dir().join("mcli.db")).unwrap(),
            before
        );
    }
}

// --- The operations (spec 005, slice 3) -------------------------------------

mod operation {
    use super::*;
    use fake_mostro::Outgoing;
    use mostro_client::commands::order::invoice::{
        bond_add_invoice, order_add_invoice, BondInvoiceOutcome, OrderInvoiceOutcome,
    };
    use mostro_client::commands::order::WriteOptions;
    use mostro_client::msg::{ExchangeLimits, PublicReadConfig, AUTO_POW_CEILING};
    use mostro_client::outcome::invoice::{
        BondResult, InvoiceInput, InvoiceKind, InvoiceRequest, InvoiceResult,
    };
    use mostro_client::outcome::{ErrorCode, Status as Outcome};
    use serde_json::{json, Value};

    const SHARE: i64 = 5_000;

    struct Holder {
        _relay: LoopbackRelay,
        fake: FakeMostro,
        ledger: Arc<Mutex<Ledger>>,
        config: PublicReadConfig,
        identity: Keys,
        sandbox: Sandbox,
        store: Store,
        buyer: Keys,
    }

    /// A user who holds orders 1 and 9 under one trade key, against a node that
    /// answers with `respond` (the invoice daemon when `None`).
    async fn holder(
        orders: impl FnOnce(PublicKey) -> Vec<Owed>,
        acks: bool,
        pow: u8,
        wrap: impl FnOnce(Respond) -> Respond,
    ) -> Holder {
        holder_with(orders, acks, (2, pow, false), wrap).await
    }

    /// A node advertising `(protocol, pow)`; its relay refuses every write when
    /// the last element is set.
    async fn holder_with(
        orders: impl FnOnce(PublicKey) -> Vec<Owed>,
        acks: bool,
        (protocol, pow, block_writes): (u8, u8, bool),
        wrap: impl FnOnce(Respond) -> Respond,
    ) -> Holder {
        let buyer = Keys::generate();
        let ledger = Ledger {
            orders: orders(buyer.public_key()),
            bonds: vec![BondOwed {
                order: id(9),
                recipient: buyer.public_key(),
                share: SHARE,
                open: true,
                invoice: None,
            }],
        };
        let (respond, ledger) = invoice_daemon(ledger, acks);
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
        let fake = FakeMostro::start(&relay.url, node, wrap(respond)).await;
        let config = PublicReadConfig::new(fake.pubkey(), &relay.url).unwrap();
        let sandbox = Sandbox::new().unwrap();
        let identity = seed_identity(&sandbox.mcli_dir()).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = Trade {
            index: 1,
            keys: buyer.clone(),
        };
        for order in [id(1), id(9)] {
            let small = Owed::waiting(order, buyer.public_key()).small(Status::WaitingBuyerInvoice);
            store.save_order(&small, &trade, REQUEST_ID).await.unwrap();
        }
        Holder {
            _relay: relay,
            fake,
            ledger,
            config,
            identity,
            sandbox,
            store,
            buyer,
        }
    }

    async fn plain() -> Holder {
        holder(waiting, true, 0, |respond| respond).await
    }

    fn input(kind: InvoiceKind, order: Uuid) -> InvoiceInput {
        let invoice = match kind {
            InvoiceKind::Order => INVOICE.to_string(),
            InvoiceKind::Bond => payout_invoice(SHARE),
        };
        InvoiceInput::validate(InvoiceRequest {
            kind,
            order,
            invoice,
        })
        .unwrap()
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

    const OPTIONS: WriteOptions = WriteOptions {
        pow: 0,
        check_protocol: true,
    };

    async fn add(holder: &Holder, order: Uuid) -> OrderInvoiceOutcome {
        order_add_invoice(
            &input(InvoiceKind::Order, order),
            &holder.config,
            &holder.identity,
            &holder.store,
            OPTIONS,
            &limits(),
        )
        .await
    }

    async fn bond(holder: &Holder, order: Uuid) -> BondInvoiceOutcome {
        bond_add_invoice(
            &input(InvoiceKind::Bond, order),
            &holder.config,
            &holder.identity,
            &holder.store,
            OPTIONS,
            &limits(),
        )
        .await
    }

    fn database(holder: &Holder) -> Vec<u8> {
        std::fs::read(holder.sandbox.mcli_dir().join("mcli.db")).unwrap()
    }

    fn stages<D: serde::Serialize>(outcome: &mostro_client::outcome::Envelope<D, ()>) -> Value {
        let mut meta = serde_json::to_value(outcome).unwrap()["meta"].clone();
        meta.as_object_mut().unwrap().remove("snapshot");
        meta
    }

    fn meta(publication: &str, acceptance: &str, persistence: &str) -> Value {
        json!({"publication": publication, "daemon_acceptance": acceptance,
               "persistence": persistence})
    }

    async fn status_of(holder: &Holder, order: Uuid) -> String {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(holder.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let value = sqlx::query_scalar("SELECT status FROM orders WHERE id = ?1")
            .bind(order.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    async fn last_index(holder: &Holder) -> i64 {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(holder.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let value = sqlx::query_scalar("SELECT last_trade_index FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    /// Decoys the node sends before its real answer.
    fn with_decoys(decoys: Vec<Outgoing>) -> impl FnOnce(Respond) -> Respond {
        move |inner| {
            Arc::new(move |request: &fake_mostro::Received| {
                let mut out = decoys.clone();
                out.extend(inner(request));
                out
            })
        }
    }

    fn reply_with(action: Action, order: Option<Uuid>, request_id: Option<u64>) -> Message {
        Message::new_order(order, request_id, None, action, None)
    }

    // --- order.add-invoice ---

    #[tokio::test]
    async fn an_invoice_for_a_waiting_order_is_accepted_and_the_local_status_follows() {
        let holder = plain().await;
        let index = last_index(&holder).await;

        let outcome = add(&holder, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.result(), InvoiceResult::WaitingSellerPayment);
        assert_eq!(data.order_id(), id(1).to_string());
        assert_eq!(data.trade_pubkey(), holder.buyer.public_key().to_hex());
        assert!(data.order().is_none());
        assert_eq!(stages(&outcome), meta("published", "accepted", "applied"));
        assert_eq!(status_of(&holder, id(1)).await, "waiting-payment");
        assert_eq!(status_of(&holder, id(9)).await, "waiting-buyer-invoice");
        assert_eq!(last_index(&holder).await, index, "no index is spent");
        let seen = holder.fake.received();
        assert_eq!(seen.len(), 1, "one publication");
        assert_eq!(seen[0].trade_index, None);
        assert_eq!(seen[0].order_id, Some(id(1)));
        assert_eq!(seen[0].invoice.as_deref(), Some(INVOICE));
        assert_eq!(seen[0].author, holder.buyer.public_key());
        assert_eq!(seen[0].identity, holder.identity.public_key());
        assert!(seen[0].signature_valid);
    }

    #[tokio::test]
    async fn a_seller_who_already_paid_makes_the_order_active_and_returns_it() {
        let holder = holder(
            |buyer| {
                vec![Owed {
                    preimage: true,
                    ..Owed::waiting(id(1), buyer)
                }]
            },
            true,
            0,
            |respond| respond,
        )
        .await;

        let outcome = add(&holder, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.result(), InvoiceResult::PaymentAccepted);
        assert_eq!(data.order().expect("order").id(), id(1).to_string());
        assert_eq!(stages(&outcome), meta("published", "accepted", "applied"));
        assert_eq!(status_of(&holder, id(1)).await, "active");
    }

    #[tokio::test]
    async fn a_replacement_invoice_changes_nothing_locally() {
        let holder = holder(
            |buyer| {
                vec![Owed {
                    status: Status::SettledHoldInvoice,
                    ..Owed::waiting(id(1), buyer)
                }]
            },
            true,
            0,
            |respond| respond,
        )
        .await;
        let before = database(&holder);

        let outcome = add(&holder, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(
            outcome.data().expect("data").result(),
            InvoiceResult::InvoiceUpdated
        );
        assert_eq!(
            stages(&outcome),
            meta("published", "accepted", "not_attempted")
        );
        assert_eq!(database(&holder), before, "no byte changed");
    }

    #[tokio::test]
    async fn every_refusal_is_an_error_with_the_nodes_reason_and_changes_nothing() {
        type Orders = Box<dyn Fn(PublicKey) -> Vec<Owed>>;
        let cases: Vec<(&str, Orders, &str)> = vec![
            (
                "an order that is not waiting",
                Box::new(|buyer| {
                    vec![Owed {
                        status: Status::Active,
                        ..Owed::waiting(id(1), buyer)
                    }]
                }),
                "not_allowed_by_status",
            ),
            (
                "an order of another buyer",
                Box::new(|_| vec![Owed::waiting(id(1), Keys::generate().public_key())]),
                "invalid_peer",
            ),
            (
                "an order the node does not know",
                Box::new(|_| Vec::new()),
                "not_found",
            ),
        ];
        for (label, orders, reason) in cases {
            let holder = holder(orders, true, 0, |respond| respond).await;
            let before = database(&holder);

            let outcome = add(&holder, id(1)).await;

            assert_eq!(outcome.status(), Outcome::Error, "{label}");
            assert_eq!(
                outcome.error_code(),
                Some(ErrorCode::NodeRefused),
                "{label}"
            );
            let document = serde_json::to_value(&outcome).unwrap();
            assert_eq!(document["error"]["reason"], reason, "{label}");
            assert_eq!(
                stages(&outcome),
                meta("published", "rejected", "not_attempted"),
                "{label}"
            );
            assert_eq!(database(&holder), before, "{label}");
            assert_eq!(holder.fake.received().len(), 1, "{label}: never retried");
        }
    }

    #[tokio::test]
    async fn silence_is_uncertain_and_changes_nothing() {
        let holder = holder(waiting, true, 0, |_| fake_mostro::silent()).await;
        let before = database(&holder);

        let outcome = add(&holder, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReplyTimeout));
        assert!(outcome.data().is_none());
        assert_eq!(
            stages(&outcome),
            meta("published", "unknown", "not_attempted")
        );
        assert_eq!(holder.fake.received().len(), 1, "never retried");
        assert_eq!(database(&holder), before);
    }

    #[tokio::test]
    async fn replies_that_do_not_fit_are_uncertain_and_change_nothing() {
        let text = |action: Action| {
            move |request: &fake_mostro::Received| {
                vec![Outgoing::to_requester(Message::new_order(
                    request.order_id,
                    request.request_id,
                    None,
                    action.clone(),
                    Some(Payload::TextMessage("unexpected".to_string())),
                ))]
            }
        };
        let cases: Vec<(&str, Respond)> = vec![
            (
                "a waiting reply with a payload",
                Arc::new(text(Action::WaitingSellerToPay)),
            ),
            (
                "an accepted payment without an order",
                Arc::new(text(Action::HoldInvoicePaymentAccepted)),
            ),
            (
                "an update with a payload",
                Arc::new(text(Action::InvoiceUpdated)),
            ),
            (
                "a waiting reply naming another order",
                Arc::new(|request: &fake_mostro::Received| {
                    vec![Outgoing::to_requester(reply_with(
                        Action::WaitingSellerToPay,
                        Some(id(77)),
                        request.request_id,
                    ))]
                }),
            ),
            (
                "an update naming another order",
                Arc::new(|request: &fake_mostro::Received| {
                    vec![Outgoing::to_requester(reply_with(
                        Action::InvoiceUpdated,
                        Some(id(77)),
                        request.request_id,
                    ))]
                }),
            ),
            (
                "an update naming no order",
                Arc::new(|request: &fake_mostro::Received| {
                    vec![Outgoing::to_requester(reply_with(
                        Action::InvoiceUpdated,
                        None,
                        request.request_id,
                    ))]
                }),
            ),
            (
                "an accepted payment for another order",
                Arc::new(|request: &fake_mostro::Received| {
                    let other =
                        Owed::waiting(id(77), Keys::generate().public_key()).small(Status::Active);
                    vec![Outgoing::to_requester(Message::new_order(
                        Some(id(77)),
                        request.request_id,
                        None,
                        Action::HoldInvoicePaymentAccepted,
                        Some(Payload::Order(other)),
                    ))]
                }),
            ),
        ];
        for (label, respond) in cases {
            let holder = holder(waiting, true, 0, move |_| respond).await;
            let before = database(&holder);

            let outcome = add(&holder, id(1)).await;

            assert_eq!(outcome.status(), Outcome::Uncertain, "{label}");
            assert_eq!(
                outcome.error_code(),
                Some(ErrorCode::ReplyMalformed),
                "{label}"
            );
            assert_eq!(
                stages(&outcome),
                meta("published", "unknown", "not_attempted"),
                "{label}"
            );
            assert_eq!(database(&holder), before, "{label}");
        }
    }

    #[tokio::test]
    async fn decoys_are_ignored_and_the_real_answer_is_used() {
        let decoys = vec![
            // A stale request id.
            Outgoing::to_requester(reply_with(Action::InvoiceUpdated, Some(id(1)), Some(1))),
            // Another author.
            Outgoing::to_requester(reply_with(
                Action::InvoiceUpdated,
                Some(id(1)),
                Some(REQUEST_ID),
            ))
            .signed_by(Keys::generate()),
            // Another addressee.
            Outgoing::to_requester(reply_with(
                Action::InvoiceUpdated,
                Some(id(1)),
                Some(REQUEST_ID),
            ))
            .addressed_to(Keys::generate().public_key()),
            // No request id: only the bond ack may do that.
            Outgoing::to_requester(reply_with(Action::InvoiceUpdated, Some(id(1)), None)),
        ];
        let holder = holder(waiting, true, 0, with_decoys(decoys)).await;

        let outcome = add(&holder, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(
            outcome.data().expect("data").result(),
            InvoiceResult::WaitingSellerPayment,
            "the real answer, not a decoy"
        );
    }

    #[tokio::test]
    async fn a_failed_status_update_still_reports_the_accepted_invoice_with_a_warning() {
        let holder = plain().await;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(holder.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
             BEGIN SELECT RAISE(ABORT, 'frozen'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let outcome = add(&holder, id(1)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(
            outcome.data().expect("data").result(),
            InvoiceResult::WaitingSellerPayment
        );
        assert_eq!(stages(&outcome), meta("published", "accepted", "failed"));
        let document = serde_json::to_value(&outcome).unwrap();
        assert_eq!(document["warnings"][0]["code"], "local_status_stale");
        assert_eq!(status_of(&holder, id(1)).await, "waiting-buyer-invoice");
        assert_eq!(holder.fake.received().len(), 1, "never retried");
    }

    #[tokio::test]
    async fn an_order_with_no_local_row_or_key_sends_nothing() {
        let holder = plain().await;
        let before = database(&holder);

        let unknown = add(&holder, id(2)).await;

        assert_eq!(unknown.status(), Outcome::Error);
        assert_eq!(unknown.error_code(), Some(ErrorCode::OrderUnknown));
        assert_eq!(
            stages(&unknown),
            meta("not_attempted", "unknown", "not_attempted")
        );
        assert!(holder.fake.received().is_empty());
        assert_eq!(database(&holder), before);

        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(holder.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("UPDATE orders SET trade_keys = NULL")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        let keyless = add(&holder, id(1)).await;
        assert_eq!(keyless.error_code(), Some(ErrorCode::OrderUnknown));
        assert!(holder.fake.received().is_empty());
    }

    #[tokio::test]
    async fn work_the_node_expects_stops_both_commands_before_anything_is_sent() {
        let holder = holder(waiting, true, AUTO_POW_CEILING + 1, |respond| respond).await;
        let before = database(&holder);

        let order = add(&holder, id(1)).await;
        let bond = bond(&holder, id(9)).await;

        assert_eq!(order.error_code(), Some(ErrorCode::PowRequired));
        assert_eq!(bond.error_code(), Some(ErrorCode::PowRequired));
        for status in [order.status(), bond.status()] {
            assert_eq!(status, Outcome::Error);
        }
        assert_eq!(
            stages(&order),
            meta("not_attempted", "unknown", "not_attempted")
        );
        assert!(holder.fake.received().is_empty());
        assert_eq!(database(&holder), before);
    }

    // --- bond.add-invoice ---

    #[tokio::test]
    async fn an_acknowledged_bond_invoice_is_accepted_though_the_ack_has_no_request_id() {
        let holder = plain().await;
        let before = database(&holder);

        let outcome = bond(&holder, id(9)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.result(), BondResult::InvoiceAccepted);
        assert_eq!(data.counterparty_share_sats(), Some("5000"));
        assert_eq!(data.order_id(), id(9).to_string());
        assert_eq!(data.trade_pubkey(), holder.buyer.public_key().to_hex());
        assert_eq!(
            stages(&outcome),
            meta("published", "accepted", "not_attempted")
        );
        assert_eq!(database(&holder), before, "a bond payout writes nothing");
        let seen = holder.fake.received();
        assert_eq!((seen.len(), seen[0].trade_index), (1, None));
        assert_eq!(seen[0].invoice, Some(payout_invoice(SHARE)));
        assert!(!holder.ledger.lock().unwrap().bonds[0].open);
    }

    #[tokio::test]
    async fn an_older_nodes_silence_is_uncertain_even_though_it_accepted() {
        let holder = holder(waiting, false, 0, |respond| respond).await;

        let outcome = bond(&holder, id(9)).await;

        assert_eq!(outcome.status(), Outcome::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReplyTimeout));
        assert_eq!(
            stages(&outcome),
            meta("published", "unknown", "not_attempted")
        );
        assert!(
            !holder.ledger.lock().unwrap().bonds[0].open,
            "the node did take it: that is why the status is not an error"
        );
        assert_eq!(holder.fake.received().len(), 1, "never retried");
    }

    #[tokio::test]
    async fn bond_refusals_carry_the_nodes_reason() {
        let cases: Vec<(&str, Uuid, i64, &str)> = vec![
            ("another amount", id(9), SHARE + 1, "invalid_invoice"),
            (
                "an order with no bond",
                id(1),
                SHARE,
                "not_allowed_by_status",
            ),
        ];
        for (label, order, share, reason) in cases {
            let holder = plain().await;
            holder.ledger.lock().unwrap().bonds[0].share = share;
            let before = database(&holder);

            let outcome = bond(&holder, order).await;

            assert_eq!(outcome.status(), Outcome::Error, "{label}");
            let document = serde_json::to_value(&outcome).unwrap();
            assert_eq!(document["error"]["code"], "node_refused", "{label}");
            assert_eq!(document["error"]["reason"], reason, "{label}");
            assert_eq!(database(&holder), before, "{label}");
        }
    }

    #[tokio::test]
    async fn bond_decoys_and_unusable_acks_are_not_taken_for_the_answer() {
        let ack = |order: Option<Uuid>, request_id: Option<u64>| {
            Outgoing::to_requester(reply_with(Action::BondInvoiceAccepted, order, request_id))
        };
        // Wrong order, no order, and a stale request id are all ignored; the
        // real ack still arrives.
        let decoys = vec![
            ack(Some(id(77)), None),
            ack(None, None),
            ack(Some(id(9)), Some(1)),
        ];
        let holder = holder(waiting, true, 0, with_decoys(decoys)).await;
        let outcome = bond(&holder, id(9)).await;
        assert_eq!(
            outcome.status(),
            Outcome::Ok,
            "{:?}",
            serde_json::to_value(&outcome)
        );

        // Only a decoy, no real answer: the wait times out.
        let only_decoy: Respond = Arc::new(move |_| vec![ack(Some(id(77)), None)]);
        let holder = self::holder(waiting, true, 0, move |_| only_decoy).await;
        let outcome = bond(&holder, id(9)).await;
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReplyTimeout));

        // The right order but a payload that is not an order: unusable.
        let unusable: Respond = Arc::new(|_| {
            vec![Outgoing::to_requester(Message::new_order(
                Some(id(9)),
                None,
                None,
                Action::BondInvoiceAccepted,
                Some(Payload::TextMessage("unexpected".to_string())),
            ))]
        });
        let holder = self::holder(waiting, true, 0, move |_| unusable).await;
        let outcome = bond(&holder, id(9)).await;
        assert_eq!(outcome.status(), Outcome::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReplyMalformed));
    }

    #[tokio::test]
    async fn a_bond_for_an_order_the_user_does_not_hold_sends_nothing() {
        let holder = plain().await;

        let outcome = bond(&holder, id(2)).await;

        assert_eq!(outcome.error_code(), Some(ErrorCode::OrderUnknown));
        assert!(holder.fake.received().is_empty());
    }

    // --- The binary (slice 4b) ---

    use harness::{run_with_watchdog, Captured, WATCHDOG};
    use std::process::Stdio;

    const ORDER: &str = "00000000-0000-0000-0000-000000000001";
    const BOND_ORDER: &str = "00000000-0000-0000-0000-000000000009";
    const SENTINEL_INVOICE: &str = "lnbcrt1buyerinvoiceSENTINEL";

    /// `<front> order add-invoice --order ORDER --invoice SENTINEL <tail>`.
    fn order_args<'a>(front: &[&'a str], tail: &[&'a str]) -> Vec<&'a str> {
        let mut all: Vec<&str> = front.to_vec();
        all.extend(["order", "add-invoice", "--order", ORDER]);
        all.extend(["--invoice", SENTINEL_INVOICE]);
        all.extend(tail);
        all
    }

    fn bond_args<'a>(front: &[&'a str], invoice: &'a str, tail: &[&'a str]) -> Vec<&'a str> {
        let mut all: Vec<&str> = front.to_vec();
        all.extend(["bond", "add-invoice", "--order", BOND_ORDER]);
        all.extend(["--invoice", invoice]);
        all.extend(tail);
        all
    }

    fn command(holder: &Holder, args: &[&str]) -> tokio::process::Command {
        let mut command = holder.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", holder.config.mostro_pubkey().to_hex())
            .env("RELAYS", &holder._relay.url);
        command
    }

    async fn run(holder: &Holder, args: &[&str]) -> Captured {
        let run = run_with_watchdog(command(holder, args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    /// Run with `answer` as the whole of stdin: a file the test wrote.
    async fn run_answering(holder: &Holder, args: &[&str], answer: &str) -> Captured {
        let path = std::env::temp_dir().join(format!(
            "mcli-answer-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::write(&path, answer).unwrap();
        let mut command = command(holder, args);
        command.stdin(Stdio::from(std::fs::File::open(&path).unwrap()));
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    fn document(run: &Captured) -> Value {
        let stdout = run.stdout_text();
        assert!(
            stdout.ends_with('\n') && stdout.lines().count() == 1,
            "{stdout}"
        );
        serde_json::from_str(&stdout).unwrap()
    }

    fn snapshot(holder: &Holder) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(holder.sandbox.mcli_dir())
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

    fn conforms_to_the_checked_in_schema(kind: &str, outcome: &Value) {
        let path = format!(
            "{}/docs/schemas/{kind}-add-invoice-v1.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(
            schema_check::conforms(&schema, &schema, outcome),
            "{outcome}"
        );
    }

    #[tokio::test]
    async fn order_add_invoice_in_json_is_accepted_and_nothing_secret_is_shown() {
        let holder = plain().await;
        let index = last_index(&holder).await;

        let run = run(
            &holder,
            &order_args(&["--output", "json", "--verbose"], &["--yes"]),
        )
        .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(run.stderr_text(), "verbose: order.add-invoice ok\n");
        let outcome = document(&run);
        conforms_to_the_checked_in_schema("order", &outcome);
        assert_eq!(outcome["command"], "order.add-invoice");
        assert_eq!(outcome["status"], "ok");
        assert_eq!(outcome["data"]["result"], "waiting_seller_payment");
        assert_eq!(outcome["data"]["order_id"], ORDER);
        assert_eq!(outcome["data"]["order"], Value::Null);
        assert_eq!(
            outcome["meta"],
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "applied", "snapshot": null})
        );
        assert_eq!(holder.fake.received().len(), 1);
        assert_eq!(
            holder.fake.received()[0].invoice.as_deref(),
            Some(SENTINEL_INVOICE)
        );
        assert_eq!(status_of(&holder, id(1)).await, "waiting-payment");
        assert_eq!(last_index(&holder).await, index, "no trade index is spent");
        let secret = holder.buyer.secret_key().to_secret_hex();
        for stream in [run.stdout_text(), run.stderr_text()] {
            assert!(!stream.contains(&secret), "the trade key leaked");
            assert!(!stream.contains("SENTINEL"), "the invoice came back");
            assert!(!stream.contains("127.0.0.1"), "a relay address leaked");
        }
    }

    #[tokio::test]
    async fn bond_add_invoice_in_json_is_accepted_and_writes_nothing() {
        let holder = plain().await;
        let before = snapshot(&holder);
        let invoice = payout_invoice(SHARE);

        let run = run(
            &holder,
            &bond_args(&["--output", "json", "--verbose"], &invoice, &["--yes"]),
        )
        .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(run.stderr_text(), "verbose: bond.add-invoice ok\n");
        let outcome = document(&run);
        conforms_to_the_checked_in_schema("bond", &outcome);
        assert_eq!(outcome["command"], "bond.add-invoice");
        assert_eq!(outcome["data"]["result"], "invoice_accepted");
        assert_eq!(outcome["data"]["counterparty_share_sats"], "5000");
        assert_eq!(
            outcome["meta"],
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "not_attempted", "snapshot": null})
        );
        assert_eq!(snapshot(&holder), before, "a bond payout writes nothing");
        assert!(!run.stdout_text().contains(&invoice));
    }

    #[tokio::test]
    async fn json_without_yes_does_nothing_for_either_command() {
        let invoice = payout_invoice(SHARE);
        for (kind, args) in [
            ("order", order_args(&["--output", "json"], &[])),
            ("bond", bond_args(&["--output", "json"], &invoice, &[])),
        ] {
            let holder = plain().await;
            let before = snapshot(&holder);

            let run = run(&holder, &args).await;

            assert_eq!(run.code(), Some(2), "{kind}: {}", run.stderr_text());
            assert!(run.stderr.is_empty());
            let outcome = document(&run);
            conforms_to_the_checked_in_schema(kind, &outcome);
            assert_eq!(outcome["error"]["code"], "invalid_arguments");
            assert!(holder.fake.received().is_empty());
            assert_eq!(snapshot(&holder), before);
        }
    }

    #[tokio::test]
    async fn the_prompt_accepts_y_and_blank_and_declines_everything_else() {
        for answer in ["y\n", "\n", "Y\r\n"] {
            let holder = plain().await;
            let run = run_answering(&holder, &order_args(&[], &[]), answer).await;
            assert_eq!(run.code(), Some(0), "{answer:?}: {}", run.stderr_text());
            let out = run.stdout_text();
            assert!(
                out.contains("Add Invoice Preview") && out.contains("Invoice Accepted"),
                "{out}"
            );
            assert!(run.stderr_text().contains("Send this invoice? [Y/n]"));
            assert!(!out.contains("SENTINEL") && !run.stderr_text().contains("SENTINEL"));
            assert_eq!(
                status_of(&holder, id(1)).await,
                "waiting-payment",
                "{answer:?}"
            );
        }
        for answer in ["n\n", "no\n", "maybe\n", ""] {
            let holder = plain().await;
            let before = snapshot(&holder);
            let run = run_answering(&holder, &order_args(&[], &[]), answer).await;
            assert_eq!(run.code(), Some(1), "{answer:?}: {}", run.stderr_text());
            assert!(!run.stdout_text().contains("Invoice Accepted"));
            assert!(run
                .stderr_text()
                .contains("error: The order was not confirmed, so nothing was sent."));
            assert!(holder.fake.received().is_empty(), "{answer:?}");
            assert_eq!(snapshot(&holder), before, "{answer:?}");
        }
        let holder = plain().await;
        let invoice = payout_invoice(SHARE);
        let run = run_answering(&holder, &bond_args(&[], &invoice, &[]), "\n").await;
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        let out = run.stdout_text();
        assert!(
            out.contains("Bond Payout Invoice Preview") && out.contains("Your share: 5000 sats"),
            "{out}"
        );
        let closed = self::run(&holder, &order_args(&[], &[])).await;
        assert_eq!(closed.code(), Some(1), "a closed stdin is not approval");
    }

    #[tokio::test]
    async fn an_unknown_order_fails_before_any_question_is_asked() {
        let holder = plain().await;
        let before = snapshot(&holder);
        let other = "00000000-0000-0000-0000-000000000002";

        let human = run(
            &holder,
            &[
                "order",
                "add-invoice",
                "--order",
                other,
                "--invoice",
                SENTINEL_INVOICE,
            ],
        )
        .await;
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout.is_empty());
        assert_eq!(
            human.stderr_text(),
            "error: No local order has this id, so nothing was sent.\n"
        );

        let json = run(
            &holder,
            &[
                "--output",
                "json",
                "order",
                "add-invoice",
                "--order",
                other,
                "--invoice",
                SENTINEL_INVOICE,
                "--yes",
            ],
        )
        .await;
        assert_eq!(json.code(), Some(1));
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("order", &outcome);
        assert_eq!(outcome["error"]["code"], "order_unknown");
        assert!(holder.fake.received().is_empty());
        assert_eq!(snapshot(&holder), before);
    }

    #[tokio::test]
    async fn a_refusal_shows_its_reason_in_both_modes_and_changes_nothing() {
        let holder = holder(
            |buyer| {
                vec![Owed {
                    status: Status::Active,
                    ..Owed::waiting(id(1), buyer)
                }]
            },
            true,
            0,
            |respond| respond,
        )
        .await;
        let before = snapshot(&holder);

        let json = run(&holder, &order_args(&["--output", "json"], &["--yes"])).await;
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("order", &outcome);
        assert_eq!(outcome["error"]["code"], "node_refused");
        assert_eq!(outcome["error"]["reason"], "not_allowed_by_status");
        assert_eq!(outcome["data"], Value::Null);

        let human = run(&holder, &order_args(&[], &["--yes"])).await;
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout.is_empty());
        assert_eq!(
            human.stderr_text(),
            "error: The node refused the request. (reason: not_allowed_by_status)\n"
        );
        assert_eq!(snapshot(&holder), before);
        assert_eq!(holder.fake.received().len(), 2, "one request per run");
    }

    #[tokio::test]
    async fn a_silent_node_is_uncertain_after_the_production_budget_and_is_never_retried() {
        let holder = holder(waiting, false, 0, |respond| respond).await;
        let invoice = payout_invoice(SHARE);
        let started = std::time::Instant::now();

        let json = run(
            &holder,
            &bond_args(&["--output", "json"], &invoice, &["--yes"]),
        )
        .await;

        assert!(started.elapsed() >= ExchangeLimits::default().reply);
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        assert!(json.stderr.is_empty());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("bond", &outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "reply_timeout");
        assert_eq!(outcome["meta"]["persistence"], "not_attempted");
        assert_eq!(holder.fake.received().len(), 1, "never retried");

        // The older node took the first invoice, so a second run needs a fresh node.
        let holder = self::holder(waiting, false, 0, |respond| respond).await;
        let human = run(&holder, &bond_args(&[], &invoice, &["--yes"])).await;
        assert_eq!(human.code(), Some(1));
        let err = human.stderr_text();
        assert!(
            err.contains("error: The node did not answer before the deadline."),
            "{err}"
        );
        assert!(err.contains("an older node never confirms it"), "{err}");
    }

    #[tokio::test]
    async fn a_stale_local_status_is_a_warning_and_the_exit_is_still_success() {
        let holder = plain().await;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(holder.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
             BEGIN SELECT RAISE(ABORT, 'frozen'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let run = run(&holder, &order_args(&["--output", "json"], &["--yes"])).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        let outcome = document(&run);
        conforms_to_the_checked_in_schema("order", &outcome);
        assert_eq!(outcome["status"], "ok");
        assert_eq!(outcome["meta"]["persistence"], "failed");
        assert_eq!(outcome["warnings"][0]["code"], "local_status_stale");
    }

    #[tokio::test]
    async fn invalid_invoices_are_rejected_before_anything_happens_without_echoing_them() {
        let holder = plain().await;
        let before = snapshot(&holder);
        let secret = "SENTINEL value";

        let order = run(
            &holder,
            &[
                "--output",
                "json",
                "order",
                "add-invoice",
                "--order",
                ORDER,
                "--invoice",
                secret,
                "--yes",
            ],
        )
        .await;
        let bond = run(
            &holder,
            &bond_args(&["--output", "json"], "buyer@example.com", &["--yes"]),
        )
        .await;

        for (kind, run) in [("order", order), ("bond", bond)] {
            assert_eq!(run.code(), Some(2), "{kind}: {}", run.stderr_text());
            let outcome = document(&run);
            conforms_to_the_checked_in_schema(kind, &outcome);
            assert_eq!(outcome["error"]["code"], "invalid_arguments");
            for stream in [run.stdout_text(), run.stderr_text()] {
                assert!(
                    !stream.contains("SENTINEL") && !stream.contains("buyer@"),
                    "{stream}"
                );
            }
        }
        assert!(holder.fake.received().is_empty());
        assert_eq!(snapshot(&holder), before);
    }

    // --- The failure matrix (slice 5) ---

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
        NoOrdersTable,
        ReadOnly,
    }

    async fn tweak(holder: &Holder, how: Tweak) {
        let path = holder.sandbox.mcli_dir().join("mcli.db");
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
            Tweak::NoOrdersTable => sql("DROP TABLE orders").await,
            Tweak::ReadOnly => {
                let mut permissions = std::fs::metadata(&path).unwrap().permissions();
                permissions.set_readonly(true);
                std::fs::set_permissions(&path, permissions).unwrap();
            }
        }
    }

    /// Whether the process can still write a file marked read-only (root).
    fn ignores_read_only(holder: &Holder) -> bool {
        std::fs::OpenOptions::new()
            .write(true)
            .open(holder.sandbox.mcli_dir().join("mcli.db"))
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
        /// Which commands the case applies to.
        kinds: &'static [&'static str],
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
                kinds: &["order", "bond"],
            }
        }
    }

    async fn run_env(
        holder: &Holder,
        args: &[&str],
        set: &[(&str, &str)],
        unset: &[&str],
    ) -> Captured {
        let mut command = command(holder, args);
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

    /// The arguments of `kind` with `front` before the command and `--yes`.
    fn kind_args<'a>(kind: &str, front: &[&'a str], payout: &'a str) -> Vec<&'a str> {
        match kind {
            "order" => order_args(front, &["--yes"]),
            _ => bond_args(front, payout, &["--yes"]),
        }
    }

    /// Run `case` as JSON, human and verbose, on both commands, and check what
    /// every mode shows, then that nothing was sent and nothing changed.
    async fn check_failure(case: &Failing, holder: &Holder) {
        tweak(holder, case.tweak).await;
        let before = snapshot(holder);
        let label = case.label;
        let payout = payout_invoice(SHARE);
        for kind in case.kinds {
            for mode in [&["--output", "json"][..], &[][..], &["--verbose"][..]] {
                let mut front: Vec<&str> = case.flags.to_vec();
                front.extend(mode);
                let run = run_env(
                    holder,
                    &kind_args(kind, &front, &payout),
                    case.set,
                    case.unset,
                )
                .await;
                assert_eq!(
                    run.code(),
                    Some(1),
                    "{label} {kind} {mode:?}: {}",
                    run.stderr_text()
                );
                let (stdout, stderr) = (run.stdout_text(), run.stderr_text());
                for stream in [&stdout, &stderr] {
                    assert!(!stream.contains("SENTINEL"), "{label}: a sentinel leaked");
                    assert!(!stream.contains(&payout), "{label}: the invoice leaked");
                    assert!(
                        !stream.contains("127.0.0.1"),
                        "{label}: a relay address leaked"
                    );
                }
                if mode == ["--output", "json"] {
                    let outcome = document(&run);
                    conforms_to_the_checked_in_schema(kind, &outcome);
                    assert_eq!(outcome["command"], format!("{kind}.add-invoice"), "{label}");
                    assert_eq!(outcome["status"], "error", "{label}");
                    assert_eq!(outcome["data"], Value::Null, "{label}");
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
                        !stderr.contains("may hold this invoice"),
                        "{label}: not uncertain"
                    );
                }
            }
        }
        assert!(
            holder.fake.received().is_empty(),
            "{label}: something was sent"
        );
        if !matches!(case.tweak, Tweak::ReadOnly) {
            assert_eq!(snapshot(holder), before, "{label}: the store changed");
        }
        assert!(
            harness::is_empty_dir(&holder.sandbox.home())
                && harness::is_empty_dir(&holder.sandbox.cwd()),
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
            let holder = plain().await;
            check_failure(case, &holder).await;
        }
    }

    #[tokio::test]
    async fn an_unqueryable_orders_table_fails_both_commands_within_their_schemas() {
        // The identity stays readable, but the order's key cannot be looked up.
        let holder = plain().await;
        check_failure(
            &Failing::new("no orders table", Tweak::NoOrdersTable, "store_unwritable"),
            &holder,
        )
        .await;
    }

    #[tokio::test]
    async fn the_human_preview_reports_a_broken_store_not_an_unknown_order() {
        // Without --yes the order's key is looked up for the preview; a store
        // that cannot be queried gives the same code as the send path.
        let holder = plain().await;
        tweak(&holder, Tweak::NoOrdersTable).await;

        let run = run(&holder, &order_args(&[], &[])).await;

        let stderr = run.stderr_text();
        assert_eq!(run.code(), Some(1), "{stderr}");
        assert!(stderr.contains("cannot be written"), "{stderr}");
        assert!(!stderr.contains("No local order"), "{stderr}");
        assert!(holder.fake.received().is_empty(), "nothing was sent");
    }

    #[tokio::test]
    async fn only_the_order_invoice_needs_a_writable_store() {
        let holder = plain().await;
        tweak(&holder, Tweak::ReadOnly).await;
        if ignores_read_only(&holder) {
            return; // running as a user the file mode does not bind
        }
        let case = Failing {
            kinds: &["order"],
            ..Failing::new("read-only store", Tweak::None, "store_unwritable")
        };
        check_failure(&case, &holder).await;

        // A bond payout writes nothing, so a read-only store does not stop it.
        let invoice = payout_invoice(SHARE);
        let run = run(
            &holder,
            &bond_args(&["--output", "json"], &invoice, &["--yes"]),
        )
        .await;
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(document(&run)["status"], "ok");
    }

    #[tokio::test]
    async fn a_node_that_cannot_be_served_fails_before_publishing() {
        let old = holder_with(waiting, true, (1, 0, false), |respond| respond).await;
        check_failure(
            &Failing::new("protocol 1", Tweak::None, "protocol_unsupported"),
            &old,
        )
        .await;

        let work = holder_with(waiting, true, (2, AUTO_POW_CEILING + 1, false), |respond| {
            respond
        })
        .await;
        check_failure(
            &Failing::new("more work expected", Tweak::None, "pow_required"),
            &work,
        )
        .await;

        // Work below the ceiling is mined, without `--pow`.
        let mined = holder_with(waiting, true, (2, 9, false), |respond| respond).await;
        let added = run(&mined, &order_args(&["--output", "json"], &["--yes"])).await;
        assert_eq!(added.code(), Some(0), "{}", added.stderr_text());
        assert!(mined.fake.received()[0].work >= 9);

        // An explicit transport skips the protocol check and enough work passes.
        let holder = holder_with(waiting, true, (1, 9, false), |respond| respond).await;
        let run = run(
            &holder,
            &order_args(
                &["--transport", "nip44", "--pow", "9", "--output", "json"],
                &["--yes"],
            ),
        )
        .await;
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    }

    #[tokio::test]
    async fn a_relay_that_rejects_the_request_changes_nothing() {
        let holder = holder_with(waiting, true, (2, 0, true), |respond| respond).await;
        check_failure(
            &Failing::new("rejected", Tweak::None, "request_rejected"),
            &holder,
        )
        .await;
    }

    #[tokio::test]
    async fn an_unreachable_relay_is_a_connection_failure_and_costs_nothing() {
        let holder = plain().await;
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("ws://{}", free.local_addr().unwrap());
        drop(free);
        let before = snapshot(&holder);

        let run = run_env(
            &holder,
            &order_args(&["--output", "json"], &["--yes"]),
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
        assert_eq!(snapshot(&holder), before);
    }

    #[tokio::test]
    async fn invalid_input_is_an_argument_error_before_any_effect() {
        let holder = plain().await;
        let before = snapshot(&holder);
        let huge = "ln".to_string() + &"x".repeat(5000);
        let order = json!("order.add-invoice");
        let bond = json!("bond.add-invoice");
        let cases: Vec<(Vec<&str>, Value)> = vec![
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "add-invoice",
                    "--order",
                    ORDER,
                    "--invoice",
                    &huge,
                    "--yes",
                ],
                order.clone(),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "add-invoice",
                    "--order",
                    ORDER,
                    "--invoice",
                    "SENTINEL zq",
                    "--yes",
                ],
                order.clone(),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "add-invoice",
                    "--order",
                    ORDER,
                    "--invoice",
                    "lnurl1SENTINEL",
                    "--yes",
                ],
                order.clone(),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "add-invoice",
                    "--order",
                    "not-a-uuid",
                    "--invoice",
                    "lnbc1x",
                    "--yes",
                ],
                order.clone(),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "add-invoice",
                    "--order",
                    ORDER,
                    "--yes",
                ],
                order.clone(),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "add-invoice",
                    "--invoice",
                    "lnbc1x",
                    "--yes",
                ],
                order.clone(),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "bond",
                    "add-invoice",
                    "--order",
                    BOND_ORDER,
                    "--invoice",
                    "buyer@SENTINEL.example",
                    "--yes",
                ],
                bond.clone(),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "bond",
                    "add-invoice",
                    "--order",
                    BOND_ORDER,
                    "--invoice",
                    "SENTINEL-zq81",
                    "--yes",
                ],
                bond.clone(),
            ),
            // The removed spellings have no command.
            (
                vec![
                    "--output",
                    "json",
                    "addinvoice",
                    "-o",
                    ORDER,
                    "-i",
                    "lnbc1x",
                ],
                Value::Null,
            ),
            (
                vec![
                    "--output",
                    "json",
                    "addbondinvoice",
                    "-o",
                    BOND_ORDER,
                    "-i",
                    "lnbc1x",
                ],
                Value::Null,
            ),
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "add-invoice",
                    "--order-id",
                    ORDER,
                    "--invoice",
                    "lnbc1x",
                    "--yes",
                ],
                order.clone(),
            ),
        ];
        for (arguments, command) in cases {
            let run = run(&holder, &arguments).await;
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
            for text in [run.stdout_text(), run.stderr_text()] {
                assert!(!text.contains("SENTINEL"), "{arguments:?}: {text}");
                assert!(!text.contains("zq"), "{arguments:?}");
            }
            assert!(
                run.stderr.is_empty(),
                "{arguments:?}: {}",
                run.stderr_text()
            );
        }
        assert!(holder.fake.received().is_empty());
        assert_eq!(snapshot(&holder), before);
    }

    #[tokio::test]
    async fn an_unwritable_stdout_after_the_invoice_exits_1_with_one_fixed_line() {
        for (front, line) in [
            (
                &["--output", "json"][..],
                "error: the JSON outcome could not be written\n",
            ),
            (&[][..], "error: the invoice result could not be written\n"),
        ] {
            for kind in ["order", "bond"] {
                let holder = plain().await;
                let payout = payout_invoice(SHARE);
                let all = kind_args(kind, front, &payout);
                let mut command = command(&holder, &all);
                command.stdout(harness::closed_pipe());
                let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
                assert_eq!(
                    run.code(),
                    Some(1),
                    "{kind} {front:?}: {}",
                    run.stderr_text()
                );
                assert_eq!(run.stderr_text(), line, "{kind} {front:?}");
                // The invoice was sent and the node took it all the same.
                assert_eq!(holder.fake.received().len(), 1, "{kind}");
            }
        }
    }

    #[tokio::test]
    async fn an_unwritable_stderr_changes_neither_the_exit_nor_the_document() {
        let holder = plain().await;
        let mut command = command(
            &holder,
            &order_args(&["--output", "json", "--pow", "x"], &["--yes"]),
        );
        command.stderr(harness::closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1));
        assert_eq!(document(&run)["error"]["code"], "configuration_invalid");
    }
}
