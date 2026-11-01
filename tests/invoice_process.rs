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

use fake_mostro::{
    info_event, invoice_daemon, payout_invoice, seed_identity, BondOwed, FakeMostro, Ledger, Owed,
    Respond,
};
use harness::{Backend, LoopbackRelay, Sandbox};
use mostro_client::store::{Store, StoreError, Trade};

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
    use mostro_client::msg::{ExchangeLimits, PublicReadConfig};
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
        let relay = LocalRelay::new();
        relay
            .add_event(info_event(&node, Some(2), Some(pow), None, Some(10)))
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
        let holder = holder(waiting, true, 9, |respond| respond).await;
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
}
