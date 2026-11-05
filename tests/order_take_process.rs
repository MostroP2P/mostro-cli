//! Tests for `order take-sell` and `order take-buy` (spec 004).
//!
//! Slice 1 holds the self-tests of the fixtures built for the commands: the
//! scripted daemon that applies the source's `check_trade_index`,
//! `take_sell_action` and `take_buy_action` rules and keeps a small book. The
//! commands' own scenarios are added with the slices that implement them.

use std::time::Duration;

use futures::StreamExt;
use mostro_core::order::{Kind as OrderKind, Status};
use mostro_core::prelude::{
    unwrap_incoming, wrap_message_with, Action, CantDoReason, Message, MessageKind, Payload,
    Transport, WrapOptions,
};
use nostr_sdk::prelude::{
    Client, ClientNotification, Event, Filter, Keys, Kind, LocalRelay, MachineReadablePrefix,
    PublicKey, WritePolicy, WritePolicyResult,
};
use tokio::time::timeout;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{info_event, take_daemon, Book, FakeMostro, Listed, Respond, HOLD_INVOICE};
use harness::{Backend, LoopbackRelay};

const REQUEST_ID: u64 = 6150;

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
const WAIT: Duration = Duration::from_millis(700);

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

    /// Take as `identity` with a fresh trade key; returns the replies to the trade key.
    async fn take(&self, identity: &Keys, message: Message) -> Vec<Message> {
        let trade = Keys::generate();
        self.hear(&trade, self.send(&message, identity, &trade))
            .await
    }
}

fn request(action: Action, order: Uuid, index: i64, payload: Option<Payload>) -> Message {
    Message::new_order(Some(order), Some(REQUEST_ID), Some(index), action, payload)
}

fn take_sell(order: Uuid, index: i64, invoice: Option<&str>, amount: Option<i64>) -> Message {
    let payload = match (invoice, amount) {
        (Some(invoice), amount) => Payload::PaymentRequest(None, invoice.to_string(), amount),
        (None, amount) => Payload::Amount(amount.unwrap_or(0)),
    };
    request(Action::TakeSell, order, index, Some(payload))
}

fn take_buy(order: Uuid, index: i64, amount: Option<i64>) -> Message {
    request(Action::TakeBuy, order, index, amount.map(Payload::Amount))
}

struct World {
    _relay: LoopbackRelay,
    fake: FakeMostro,
    peer: Peer,
    book: std::sync::Arc<std::sync::Mutex<Book>>,
    maker: Keys,
}

async fn world(orders: impl FnOnce(PublicKey) -> Vec<Listed>, last: i64, bonded: bool) -> World {
    let maker = Keys::generate();
    let book = Book {
        orders: orders(maker.public_key()),
        last_trade_index: last,
        ..Book::default()
    };
    let (respond, book): (Respond, _) = take_daemon(book, bonded);
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
        book,
        maker,
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

fn sell_order(maker: PublicKey) -> Vec<Listed> {
    vec![Listed::fixed(id(1), OrderKind::Sell, maker)]
}

#[tokio::test]
async fn a_take_sell_without_an_invoice_gets_the_order_to_add_an_invoice_to() {
    let world = world(sell_order, 0, false).await;

    let reply = only(
        world
            .peer
            .take(&Keys::generate(), take_sell(id(1), 1, None, None))
            .await,
    );

    assert_eq!(reply.action, Action::AddInvoice);
    assert_eq!(
        (reply.id, reply.request_id, reply.trade_index),
        (Some(id(1)), Some(REQUEST_ID), Some(1))
    );
    let Some(Payload::Order(order)) = reply.payload else {
        panic!("an order payload");
    };
    assert_eq!(order.id, Some(id(1)));
    assert_eq!(order.status, Some(Status::WaitingBuyerInvoice));
    let seen = world.fake.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].order_id, seen[0].amount, seen[0].invoice.clone()),
        (Some(id(1)), Some(0), None)
    );
    assert!(seen[0].signature_valid);
    assert_ne!(seen[0].author, seen[0].identity);
    let book = world.book.lock().unwrap();
    assert_eq!(book.last_trade_index, 1);
    assert_eq!(book.orders[0].status, Status::WaitingBuyerInvoice);
}

#[tokio::test]
async fn a_take_sell_with_an_invoice_is_only_told_to_wait_while_the_maker_gets_the_hold_invoice() {
    let world = world(sell_order, 0, false).await;
    let identity = Keys::generate();
    let trade = Keys::generate();
    let message = take_sell(id(1), 1, Some("lnbcrt1buyerinvoice"), None);

    let mut heard = world
        .peer
        .hear_all(
            &[&trade, &world.maker],
            world.peer.send(&message, &identity, &trade),
        )
        .await;
    let maker = heard.remove(1);
    let taker = heard.remove(0);

    let reply = only(taker);
    assert_eq!(reply.action, Action::WaitingSellerToPay);
    assert!(reply.payload.is_none(), "no order in this reply (#204)");
    assert_eq!(
        (reply.id, reply.request_id),
        (Some(id(1)), Some(REQUEST_ID))
    );
    let hold = only(maker);
    assert_eq!(hold.action, Action::PayInvoice);
    assert!(matches!(
        hold.payload,
        Some(Payload::PaymentRequest(Some(_), ref invoice, None)) if invoice == HOLD_INVOICE
    ));
    assert_eq!(
        world.fake.received()[0].invoice.as_deref(),
        Some("lnbcrt1buyerinvoice")
    );
    assert_eq!(
        world.book.lock().unwrap().orders[0].status,
        Status::WaitingPayment
    );
}

#[tokio::test]
async fn a_take_buy_gets_the_hold_invoice_to_pay() {
    let world = world(
        |maker| vec![Listed::fixed(id(2), OrderKind::Buy, maker)],
        0,
        false,
    )
    .await;

    let reply = only(
        world
            .peer
            .take(&Keys::generate(), take_buy(id(2), 1, None))
            .await,
    );

    assert_eq!(reply.action, Action::PayInvoice);
    let Some(Payload::PaymentRequest(Some(order), invoice, None)) = reply.payload else {
        panic!("a payment request with the order");
    };
    assert_eq!(invoice, HOLD_INVOICE);
    assert_eq!(
        (order.id, order.status),
        (Some(id(2)), Some(Status::WaitingPayment))
    );
    assert_eq!(world.fake.received()[0].amount, None);
}

#[tokio::test]
async fn a_range_order_needs_an_amount_inside_its_bounds() {
    let world = world(
        |maker| vec![Listed::fixed(id(3), OrderKind::Sell, maker).ranged(100, 500)],
        0,
        false,
    )
    .await;
    let taker = Keys::generate();

    for amount in [None, Some(50), Some(501)] {
        let kind = only(
            world
                .peer
                .take(&taker, take_sell(id(3), 1, None, amount))
                .await,
        );
        assert_eq!(
            reason(&kind),
            CantDoReason::OutOfRangeSatsAmount,
            "{amount:?}"
        );
    }
    assert_eq!(world.book.lock().unwrap().last_trade_index, 0);

    let ok = only(
        world
            .peer
            .take(&taker, take_sell(id(3), 1, None, Some(150)))
            .await,
    );
    assert_eq!(ok.action, Action::AddInvoice);
    let Some(Payload::Order(order)) = ok.payload else {
        panic!("an order");
    };
    assert_eq!(order.fiat_amount, 150, "the amount taken");
    assert_eq!((order.min_amount, order.max_amount), (Some(100), Some(500)));
}

#[tokio::test]
async fn each_refusal_follows_the_source_and_spends_no_index() {
    let world = world(
        |maker| {
            let mut taken = Listed::fixed(id(5), OrderKind::Sell, maker);
            taken.status = Status::WaitingBuyerInvoice;
            vec![
                Listed::fixed(id(1), OrderKind::Sell, maker),
                Listed::fixed(id(4), OrderKind::Buy, maker),
                taken,
            ]
        },
        2,
        false,
    )
    .await;
    let taker = Keys::generate();
    let refused = |replies: Vec<Message>| reason(&only(replies));

    // A stale index.
    assert_eq!(
        refused(
            world
                .peer
                .take(&taker, take_sell(id(1), 2, None, None))
                .await
        ),
        CantDoReason::InvalidTradeIndex
    );
    // An order that is not in the book.
    assert_eq!(
        refused(
            world
                .peer
                .take(&taker, take_sell(id(9), 3, None, None))
                .await
        ),
        CantDoReason::NotFound
    );
    // The wrong side: take-sell on a buy order and take-buy on a sell order.
    assert_eq!(
        refused(
            world
                .peer
                .take(&taker, take_sell(id(4), 3, None, None))
                .await
        ),
        CantDoReason::InvalidOrderKind
    );
    assert_eq!(
        refused(world.peer.take(&taker, take_buy(id(1), 3, None)).await),
        CantDoReason::InvalidOrderKind
    );
    // An order that is already taken.
    assert_eq!(
        refused(
            world
                .peer
                .take(&taker, take_sell(id(5), 3, None, None))
                .await
        ),
        CantDoReason::InvalidOrderStatus
    );
    // An invalid invoice.
    assert_eq!(
        refused(
            world
                .peer
                .take(&taker, take_sell(id(1), 3, Some("garbage"), None))
                .await
        ),
        CantDoReason::InvalidInvoice
    );
    assert_eq!(
        world.book.lock().unwrap().last_trade_index,
        2,
        "no refusal spent the index"
    );
    assert!(world.book.lock().unwrap().takers.is_empty());

    // The maker's own trade key may not take the order.
    let own = world
        .peer
        .hear(
            &world.maker,
            world
                .peer
                .send(&take_sell(id(1), 3, None, None), &taker, &world.maker),
        )
        .await;
    assert_eq!(refused(own), CantDoReason::InvalidPubkey);

    // After a take, the same identity has a pending order and cannot take another.
    let ok = only(
        world
            .peer
            .take(&taker, take_sell(id(1), 3, None, None))
            .await,
    );
    assert_eq!(ok.action, Action::AddInvoice);
    assert_eq!(
        refused(world.peer.take(&taker, take_buy(id(4), 4, None)).await),
        CantDoReason::PendingOrderExists
    );
    assert_eq!(world.book.lock().unwrap().last_trade_index, 3);
}

#[tokio::test]
async fn a_bonded_node_asks_for_a_bond_instead_and_keeps_the_order_parked() {
    let world = world(sell_order, 0, true).await;

    let reply = only(
        world
            .peer
            .take(&Keys::generate(), take_sell(id(1), 1, None, None))
            .await,
    );

    assert_eq!(reply.action, Action::PayBondInvoice);
    assert!(matches!(
        reply.payload,
        Some(Payload::PaymentRequest(Some(_), _, None))
    ));
    let book = world.book.lock().unwrap();
    assert_eq!(book.last_trade_index, 1, "the node consumed the index");
    assert_eq!(book.orders[0].status, Status::WaitingTakerBond);
}

// --- The operation (spec 004, slice 3) --------------------------------------

mod operation {
    use super::*;
    use fake_mostro::{seed_identity, Outgoing};
    use harness::Sandbox;
    use mostro_client::commands::order::take::{order_take, TakeOutcome};
    use mostro_client::commands::order::WriteOptions;
    use mostro_client::msg::{ExchangeLimits, PublicReadConfig};
    use mostro_client::outcome::take::{NextStep, OrderTakeInput, TakeRequest, TakeSide};
    use mostro_client::outcome::{ErrorCode, Status as Outcome};
    use mostro_client::store::Store;
    use serde_json::json;

    struct Taker {
        relay: LoopbackRelay,
        fake: FakeMostro,
        book: std::sync::Arc<std::sync::Mutex<Book>>,
        config: PublicReadConfig,
        identity: Keys,
        sandbox: Sandbox,
        store: Store,
    }

    async fn taker(
        respond: Respond,
        book: std::sync::Arc<std::sync::Mutex<Book>>,
        pow: u8,
    ) -> Taker {
        taker_with(respond, book, 2, pow, false).await
    }

    /// A node advertising `protocol` and `pow`; its relay refuses every write
    /// when `block_writes` is set.
    async fn taker_with(
        respond: Respond,
        book: std::sync::Arc<std::sync::Mutex<Book>>,
        protocol: u8,
        pow: u8,
        block_writes: bool,
    ) -> Taker {
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
        Taker {
            relay,
            fake,
            book,
            config,
            identity,
            sandbox,
            store,
        }
    }

    async fn with_book(orders: Vec<Listed>, last: i64, bonded: bool, pow: u8) -> Taker {
        let (respond, book) = take_daemon(
            Book {
                orders,
                last_trade_index: last,
                ..Book::default()
            },
            bonded,
        );
        taker(respond, book, pow).await
    }

    fn sell_book() -> Vec<Listed> {
        vec![Listed::fixed(
            id(1),
            OrderKind::Sell,
            Keys::generate().public_key(),
        )]
    }

    fn input(
        side: TakeSide,
        order: Uuid,
        amount: Option<i64>,
        invoice: Option<&str>,
    ) -> OrderTakeInput {
        OrderTakeInput::validate(TakeRequest {
            side,
            order,
            amount,
            invoice: invoice.map(str::to_owned),
        })
        .unwrap()
    }

    async fn take(taker: &Taker, input: &OrderTakeInput) -> TakeOutcome {
        order_take(
            input,
            &taker.config,
            &taker.identity,
            &taker.store,
            WriteOptions {
                pow: 0,
                check_protocol: true,
            },
            &ExchangeLimits {
                connect: Duration::from_secs(2),
                probe: Duration::from_millis(400),
                reply: Duration::from_millis(1_500),
                cleanup: Duration::from_millis(500),
                ..ExchangeLimits::default()
            },
        )
        .await
    }

    async fn scalar<T>(taker: &Taker, sql: &'static str) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(taker.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let value = sqlx::query_scalar::<_, T>(sql)
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    async fn state(taker: &Taker) -> (i64, i64) {
        (
            scalar(taker, "SELECT COUNT(*) FROM orders").await,
            scalar(taker, "SELECT COALESCE(last_trade_index, 0) FROM users").await,
        )
    }

    /// The order rows and the raw recorded index (`None` when never set): what
    /// a failure after the reservation, which gives the index back, leaves.
    async fn raw_state(taker: &Taker) -> (i64, Option<i64>) {
        (
            scalar(taker, "SELECT COUNT(*) FROM orders").await,
            scalar(taker, "SELECT last_trade_index FROM users").await,
        )
    }

    fn stages(outcome: &TakeOutcome) -> serde_json::Value {
        serde_json::to_value(outcome).unwrap()["meta"].clone()
    }

    fn accepted_and_applied() -> serde_json::Value {
        json!({"publication": "published", "daemon_acceptance": "accepted",
               "persistence": "applied", "snapshot": null})
    }

    #[tokio::test]
    async fn a_take_sell_without_an_invoice_asks_for_one_and_saves_the_order_it_got() {
        let taker = with_book(sell_book(), 0, false, 0).await;

        let outcome = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;

        assert_eq!(
            outcome.status(),
            Outcome::Ok,
            "{:?}",
            serde_json::to_value(&outcome)
        );
        let data = outcome.data().unwrap();
        assert_eq!(data.next_step(), NextStep::AddInvoice);
        assert_eq!(data.order().unwrap().id(), id(1).to_string());
        assert_eq!((data.trade_index(), data.payment_request()), ("1", None));
        assert_eq!(stages(&outcome), accepted_and_applied());
        let seen = taker.fake.received();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            (seen[0].order_id, seen[0].trade_index),
            (Some(id(1)), Some(1))
        );
        assert!(seen[0].signature_valid);
        assert_eq!(seen[0].author.to_hex(), data.trade_pubkey());
        assert_eq!(seen[0].identity, taker.identity.public_key());
        assert_eq!(state(&taker).await, (1, 1));
        assert_eq!(
            scalar::<String>(&taker, "SELECT status FROM orders").await,
            "waiting-buyer-invoice"
        );
        assert_eq!(taker.book.lock().unwrap().last_trade_index, 1);
    }

    #[tokio::test]
    async fn a_take_sell_with_an_invoice_waits_for_the_seller_and_saves_the_minimal_row() {
        let taker = with_book(sell_book(), 0, false, 0).await;

        let outcome = take(
            &taker,
            &input(TakeSide::Sell, id(1), None, Some("lnbcrt1buyerinvoice")),
        )
        .await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        assert_eq!(data.next_step(), NextStep::WaitForSellerPayment);
        assert!(data.order().is_none() && data.payment_request().is_none());
        assert_eq!(
            taker.fake.received()[0].invoice.as_deref(),
            Some("lnbcrt1buyerinvoice")
        );
        assert_eq!(state(&taker).await, (1, 1));
        assert_eq!(
            scalar::<String>(&taker, "SELECT kind FROM orders").await,
            "sell"
        );
        assert_eq!(
            scalar::<String>(&taker, "SELECT status FROM orders").await,
            "waiting-payment"
        );
        assert_eq!(
            scalar::<String>(&taker, "SELECT fiat_code FROM orders").await,
            ""
        );
        assert!(scalar::<bool>(&taker, "SELECT trade_keys IS NOT NULL FROM orders").await);
        assert_eq!(
            scalar::<String>(&taker, "SELECT id FROM orders").await,
            id(1).to_string()
        );
    }

    #[tokio::test]
    async fn a_take_buy_returns_the_hold_invoice_and_saves_the_order() {
        let taker = with_book(
            vec![Listed::fixed(
                id(2),
                OrderKind::Buy,
                Keys::generate().public_key(),
            )],
            0,
            false,
            0,
        )
        .await;

        let outcome = take(&taker, &input(TakeSide::Buy, id(2), None, None)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().unwrap();
        assert_eq!(data.next_step(), NextStep::PayHoldInvoice);
        assert_eq!(data.payment_request(), Some(HOLD_INVOICE));
        assert_eq!(data.side(), OrderKind::Buy);
        assert_eq!(state(&taker).await, (1, 1));
        assert_eq!(
            scalar::<String>(&taker, "SELECT kind FROM orders").await,
            "buy"
        );
        // The hold invoice is data, not something kept in the store.
        assert_eq!(
            scalar::<i64>(
                &taker,
                "SELECT COUNT(*) FROM orders WHERE buyer_invoice IS NOT NULL"
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn the_amount_taken_from_a_range_order_is_what_is_saved() {
        let taker = with_book(
            vec![
                Listed::fixed(id(3), OrderKind::Sell, Keys::generate().public_key())
                    .ranged(100, 500),
            ],
            0,
            false,
            0,
        )
        .await;

        let outcome = take(&taker, &input(TakeSide::Sell, id(3), Some(150), None)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        assert_eq!(taker.fake.received()[0].amount, Some(150));
        assert_eq!(
            scalar::<i64>(&taker, "SELECT fiat_amount FROM orders").await,
            150
        );
        assert_eq!(
            scalar::<i64>(&taker, "SELECT max_amount FROM orders").await,
            500
        );
        assert!(
            !scalar::<bool>(&taker, "SELECT is_mine FROM orders").await,
            "a taken order is not the user's own"
        );
    }

    #[tokio::test]
    async fn refusals_carry_their_reason_and_write_nothing() {
        let taker = with_book(sell_book(), 5, false, 0).await;
        let stale = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;
        assert_eq!(stale.status(), Outcome::Error);
        assert_eq!(stale.error_code(), Some(ErrorCode::NodeRefused));
        assert_eq!(stale.reason(), Some("invalid_trade_index"));
        assert_eq!(
            stages(&stale),
            json!({"publication": "published", "daemon_acceptance": "rejected",
                   "persistence": "not_attempted", "snapshot": null})
        );
        assert_eq!(state(&taker).await, (0, 0), "a refusal spends no index");

        let taker = with_book(sell_book(), 0, false, 0).await;
        let wrong_side = take(&taker, &input(TakeSide::Buy, id(1), None, None)).await;
        assert_eq!(wrong_side.reason(), Some("invalid_order_kind"));
        let unknown = take(&taker, &input(TakeSide::Sell, id(9), None, None)).await;
        assert_eq!(unknown.reason(), Some("not_found"));
        assert_eq!(state(&taker).await, (0, 0));
        assert_eq!(taker.book.lock().unwrap().last_trade_index, 0);
    }

    #[tokio::test]
    async fn silence_is_uncertain_and_spends_the_index_without_a_row() {
        let (_, book) = take_daemon(Book::default(), false);
        let taker = taker(fake_mostro::silent(), book, 0).await;

        let outcome = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;

        assert_eq!(outcome.status(), Outcome::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::ReplyTimeout));
        assert!(outcome.data().is_none());
        assert_eq!(
            stages(&outcome),
            json!({"publication": "published", "daemon_acceptance": "unknown",
                   "persistence": "applied", "snapshot": null})
        );
        assert_eq!(taker.fake.received().len(), 1, "never retried");
        assert_eq!(state(&taker).await, (0, 1));
    }

    #[tokio::test]
    async fn replies_that_do_not_fit_the_take_are_uncertain() {
        let cases: Vec<(&str, TakeSide, Respond)> = vec![
            (
                "an order that is not the one taken",
                TakeSide::Sell,
                std::sync::Arc::new(|request: &fake_mostro::Received| {
                    let mut order =
                        Listed::fixed(id(77), OrderKind::Sell, Keys::generate().public_key())
                            .small(Status::WaitingBuyerInvoice, 15);
                    order.id = Some(id(77));
                    let reply = Message::new_order(
                        Some(id(77)),
                        request.request_id,
                        request.trade_index,
                        Action::AddInvoice,
                        Some(Payload::Order(order)),
                    );
                    vec![Outgoing::to_requester(reply)]
                }),
            ),
            (
                "a hold invoice for a take-sell",
                TakeSide::Sell,
                std::sync::Arc::new(|request: &fake_mostro::Received| {
                    let reply = Message::new_order(
                        request.order_id,
                        request.request_id,
                        request.trade_index,
                        Action::PayInvoice,
                        Some(Payload::PaymentRequest(
                            Some(
                                Listed::fixed(
                                    id(1),
                                    OrderKind::Sell,
                                    Keys::generate().public_key(),
                                )
                                .small(Status::WaitingPayment, 15),
                            ),
                            HOLD_INVOICE.to_string(),
                            None,
                        )),
                    );
                    vec![Outgoing::to_requester(reply)]
                }),
            ),
            (
                "a waiting reply naming another order",
                TakeSide::Sell,
                std::sync::Arc::new(|request: &fake_mostro::Received| {
                    let reply = Message::new_order(
                        Some(id(77)),
                        request.request_id,
                        request.trade_index,
                        Action::WaitingSellerToPay,
                        None,
                    );
                    vec![Outgoing::to_requester(reply)]
                }),
            ),
        ];
        for (label, side, respond) in cases {
            let (_, book) = take_daemon(Book::default(), false);
            let taker = taker(respond, book, 0).await;
            let outcome = take(&taker, &input(side, id(1), None, None)).await;
            assert_eq!(outcome.status(), Outcome::Uncertain, "{label}");
            assert_eq!(
                outcome.error_code(),
                Some(ErrorCode::ReplyMalformed),
                "{label}"
            );
            assert_eq!(
                state(&taker).await,
                (0, 1),
                "{label}: the index is spent, no row"
            );
        }
    }

    #[tokio::test]
    async fn a_bonded_node_returns_the_bond_invoice_and_saves_the_parked_order() {
        let taker = with_book(sell_book(), 0, true, 0).await;

        let outcome = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;

        assert_eq!(outcome.status(), Outcome::Ok);
        let data = outcome.data().expect("data");
        assert_eq!(data.next_step(), NextStep::PayBondInvoice);
        assert_eq!(data.payment_request(), Some("lnbcrt1bondinvoice"));
        assert!(data.order().is_some(), "the parked order is returned");
        assert_eq!(stages(&outcome), accepted_and_applied());
        assert_eq!(state(&taker).await, (1, 1), "row and index in one write");
    }

    #[tokio::test]
    async fn an_accepted_take_that_cannot_be_saved_is_uncertain_and_still_spends_the_index() {
        let taker = with_book(sell_book(), 0, false, 0).await;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(taker.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DROP TABLE orders")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let outcome = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;

        assert_eq!(outcome.status(), Outcome::Uncertain);
        assert_eq!(outcome.error_code(), Some(ErrorCode::OrderNotSaved));
        assert!(
            outcome.data().is_some(),
            "what the node asked is still returned"
        );
        assert_eq!(
            stages(&outcome),
            json!({"publication": "published", "daemon_acceptance": "accepted",
                   "persistence": "failed", "snapshot": null})
        );
        assert_eq!(
            scalar::<i64>(&taker, "SELECT last_trade_index FROM users").await,
            1
        );
    }

    #[tokio::test]
    async fn work_the_node_expects_stops_the_take_before_anything_is_sent_or_written() {
        let taker = with_book(sell_book(), 0, false, 9).await;

        let outcome = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;

        assert_eq!(outcome.status(), Outcome::Error);
        assert_eq!(outcome.error_code(), Some(ErrorCode::PowRequired));
        assert_eq!(
            stages(&outcome),
            json!({"publication": "not_attempted", "daemon_acceptance": "unknown",
                   "persistence": "not_attempted", "snapshot": null})
        );
        assert!(taker.fake.received().is_empty());
        // The index was reserved and given back: the values are restored
        // exactly (NULL included), though the file was written.
        assert_eq!(raw_state(&taker).await, (0, None));
    }

    // --- The binary (slice 4b) ------------------------------------------------

    use harness::{run_with_watchdog, Captured, WATCHDOG};
    use std::process::Stdio;

    const ORDER: &str = "00000000-0000-0000-0000-000000000001";

    fn args<'a>(front: &[&'a str], command: &'a str, tail: &[&'a str]) -> Vec<&'a str> {
        let mut all: Vec<&str> = front.to_vec();
        all.extend(["order", command, "--order", ORDER]);
        all.extend(tail);
        all
    }

    fn command(taker: &Taker, args: &[&str]) -> tokio::process::Command {
        let mut command = taker.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", taker.config.mostro_pubkey().to_hex())
            .env("RELAYS", &taker.relay.url);
        command
    }

    async fn run(taker: &Taker, args: &[&str]) -> Captured {
        let run = run_with_watchdog(command(taker, args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    /// Run with `answer` as the whole of stdin: a file the test wrote.
    async fn run_answering(taker: &Taker, args: &[&str], answer: &str) -> Captured {
        let path = std::env::temp_dir().join(format!(
            "mcli-answer-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::write(&path, answer).unwrap();
        let mut command = command(taker, args);
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

    fn snapshot(taker: &Taker) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(taker.sandbox.mcli_dir())
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

    fn conforms_to_the_checked_in_schema(side: &str, outcome: &serde_json::Value) {
        let path = format!(
            "{}/docs/schemas/order-{side}-v1.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let schema: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(
            super::schema_check::conforms(&schema, &schema, outcome),
            "{outcome}"
        );
    }

    fn buy_book() -> Vec<Listed> {
        vec![Listed::fixed(
            id(1),
            OrderKind::Buy,
            Keys::generate().public_key(),
        )]
    }

    #[tokio::test]
    async fn a_take_sell_in_json_asks_for_an_invoice_and_nothing_secret_is_shown() {
        let taker = with_book(sell_book(), 0, false, 0).await;

        let run = run(
            &taker,
            &args(&["--output", "json"], "take-sell", &["--yes"]),
        )
        .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let outcome = document(&run);
        conforms_to_the_checked_in_schema("take-sell", &outcome);
        assert_eq!(outcome["command"], "order.take-sell");
        assert_eq!(outcome["status"], "ok");
        assert_eq!(outcome["data"]["next_step"], "add_invoice");
        assert_eq!(outcome["data"]["side"], "sell");
        assert_eq!(outcome["data"]["payment_request"], serde_json::Value::Null);
        assert_eq!(outcome["data"]["trade_index"], "1");
        assert_eq!(outcome["meta"], accepted_and_applied());
        assert_eq!(taker.fake.received().len(), 1);
        assert_eq!(state(&taker).await, (1, 1));
        let secret = scalar::<String>(&taker, "SELECT trade_keys FROM orders").await;
        let mnemonic = scalar::<String>(&taker, "SELECT mnemonic FROM users").await;
        for stream in [run.stdout_text(), run.stderr_text()] {
            assert!(!stream.contains(&secret), "the trade key leaked");
            assert!(!stream.contains(&mnemonic), "the mnemonic leaked");
            assert!(!stream.contains("127.0.0.1"), "a relay address leaked");
        }
    }

    #[tokio::test]
    async fn the_invoice_the_user_gives_never_comes_back_in_any_stream() {
        let taker = with_book(sell_book(), 0, false, 0).await;
        let invoice = "lnbcrt1buyerinvoiceSENTINEL";
        let json = run(
            &taker,
            &args(
                &["--output", "json", "--verbose"],
                "take-sell",
                &["--invoice", invoice, "--yes"],
            ),
        )
        .await;
        assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
        let outcome = document(&json);
        assert_eq!(outcome["data"]["next_step"], "wait_for_seller_payment");
        assert_eq!(outcome["data"]["order"], serde_json::Value::Null);
        assert_eq!(json.stderr_text(), "verbose: order.take-sell ok\n");

        // A second taker identity is needed for a second take of a pending book.
        let taker = with_book(sell_book(), 0, false, 0).await;
        let human = run_answering(
            &taker,
            &args(&[], "take-sell", &["--invoice", invoice]),
            "y\n",
        )
        .await;
        assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
        let out = human.stdout_text();
        assert!(
            out.contains("Take Order Preview") && out.contains("Invoice: provided"),
            "{out}"
        );
        assert!(out.contains("Waiting for the seller"), "{out}");
        for stream in [
            json.stdout_text(),
            json.stderr_text(),
            out,
            human.stderr_text(),
        ] {
            assert!(
                !stream.contains("SENTINEL"),
                "the invoice came back: {stream}"
            );
        }
        assert_eq!(taker.fake.received()[0].invoice.as_deref(), Some(invoice));
        assert_eq!(
            scalar::<i64>(
                &taker,
                "SELECT COUNT(*) FROM orders WHERE buyer_invoice IS NOT NULL"
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn a_take_buy_returns_the_hold_invoice_on_stdout_only() {
        let taker = with_book(buy_book(), 0, false, 0).await;
        let json = run(
            &taker,
            &args(&["--output", "json", "--verbose"], "take-buy", &["--yes"]),
        )
        .await;
        assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("take-buy", &outcome);
        assert_eq!(outcome["command"], "order.take-buy");
        assert_eq!(outcome["data"]["next_step"], "pay_hold_invoice");
        assert_eq!(outcome["data"]["side"], "buy");
        assert_eq!(outcome["data"]["payment_request"], HOLD_INVOICE);
        assert_eq!(json.stderr_text(), "verbose: order.take-buy ok\n");

        let taker = with_book(buy_book(), 0, false, 0).await;
        let human = run(&taker, &args(&[], "take-buy", &["--yes"])).await;
        assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
        assert!(
            human.stdout_text().lines().any(|line| line == HOLD_INVOICE),
            "the hold invoice is alone on its line: {}",
            human.stdout_text()
        );
        assert!(human.stderr.is_empty());
        assert!(!human.stderr_text().contains(HOLD_INVOICE));
        assert_eq!(state(&taker).await, (1, 1));
    }

    #[tokio::test]
    async fn json_without_yes_does_nothing_for_either_side() {
        for (command, book) in [("take-sell", sell_book()), ("take-buy", buy_book())] {
            let taker = with_book(book, 0, false, 0).await;
            let before = snapshot(&taker);

            let run = run(&taker, &args(&["--output", "json"], command, &[])).await;

            assert_eq!(run.code(), Some(2), "{command}: {}", run.stderr_text());
            assert!(run.stderr.is_empty());
            let outcome = document(&run);
            conforms_to_the_checked_in_schema(command, &outcome);
            assert_eq!(outcome["command"], format!("order.{command}"));
            assert_eq!(outcome["error"]["code"], "invalid_arguments");
            assert!(taker.fake.received().is_empty());
            assert_eq!(snapshot(&taker), before);
        }
    }

    #[tokio::test]
    async fn the_prompt_accepts_y_and_blank_and_declines_everything_else() {
        for answer in ["y\n", "\n", "Y\r\n"] {
            let taker = with_book(sell_book(), 0, false, 0).await;
            let run = run_answering(&taker, &args(&[], "take-sell", &[]), answer).await;
            assert_eq!(run.code(), Some(0), "{answer:?}: {}", run.stderr_text());
            assert!(run.stdout_text().contains("Take Order Preview"));
            assert!(run.stdout_text().contains("Order Taken"));
            assert!(run.stderr_text().contains("Take this order? [Y/n]"));
            assert_eq!(state(&taker).await, (1, 1), "{answer:?}");
        }
        for answer in ["n\n", "no\n", "maybe\n", ""] {
            let taker = with_book(sell_book(), 0, false, 0).await;
            let before = snapshot(&taker);
            let run = run_answering(&taker, &args(&[], "take-sell", &[]), answer).await;
            assert_eq!(run.code(), Some(1), "{answer:?}: {}", run.stderr_text());
            assert!(!run.stdout_text().contains("Order Taken"));
            assert!(run
                .stderr_text()
                .contains("error: The order was not confirmed, so nothing was sent."));
            assert!(taker.fake.received().is_empty(), "{answer:?}");
            assert_eq!(snapshot(&taker), before, "{answer:?}");
        }
        let taker = with_book(sell_book(), 0, false, 0).await;
        let run = run(&taker, &args(&[], "take-sell", &[])).await;
        assert_eq!(run.code(), Some(1), "a closed stdin is not approval");
        assert!(taker.fake.received().is_empty());
    }

    #[tokio::test]
    async fn a_refusal_shows_its_reason_in_both_modes_and_spends_nothing() {
        let taker = with_book(sell_book(), 5, false, 0).await;
        let before = raw_state(&taker).await;

        let json = run(
            &taker,
            &args(&["--output", "json"], "take-sell", &["--yes"]),
        )
        .await;
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("take-sell", &outcome);
        assert_eq!(outcome["error"]["code"], "node_refused");
        assert_eq!(outcome["error"]["reason"], "invalid_trade_index");
        assert_eq!(outcome["data"], serde_json::Value::Null);

        let human = run(&taker, &args(&[], "take-sell", &["--yes"])).await;
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout.is_empty());
        assert_eq!(
            human.stderr_text(),
            "error: The node refused the request. (reason: invalid_trade_index)\n"
        );
        // The index of each run was reserved and given back.
        assert_eq!(raw_state(&taker).await, before);
        let seen = taker.fake.received();
        assert_eq!(seen.len(), 2, "one request per run");
        assert_eq!(
            (seen[0].trade_index, seen[1].trade_index),
            (Some(1), Some(1))
        );
    }

    #[tokio::test]
    async fn a_silent_node_is_uncertain_after_the_production_budget_and_is_never_retried() {
        let (_, book) = take_daemon(Book::default(), false);
        let taker = taker(fake_mostro::silent(), book, 0).await;
        let started = std::time::Instant::now();

        let run = run(
            &taker,
            &args(&["--output", "json"], "take-sell", &["--yes"]),
        )
        .await;

        assert!(started.elapsed() >= ExchangeLimits::default().reply);
        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        assert!(run.stderr.is_empty());
        let outcome = document(&run);
        conforms_to_the_checked_in_schema("take-sell", &outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "reply_timeout");
        assert_eq!(outcome["meta"]["persistence"], "applied");
        assert_eq!(taker.fake.received().len(), 1, "one publication, no retry");
        assert_eq!(state(&taker).await, (0, 1));
    }

    #[tokio::test]
    async fn a_bonded_node_gives_the_bond_invoice_to_pay() {
        let taker = with_book(sell_book(), 0, true, 0).await;

        let run = run(
            &taker,
            &args(&["--output", "json"], "take-sell", &["--yes"]),
        )
        .await;

        assert_eq!(run.code(), Some(0));
        let outcome = document(&run);
        conforms_to_the_checked_in_schema("take-sell", &outcome);
        assert_eq!(outcome["status"], "ok");
        assert_eq!(outcome["data"]["next_step"], "pay_bond_invoice");
        assert_eq!(outcome["data"]["payment_request"], "lnbcrt1bondinvoice");
        assert_eq!(state(&taker).await, (1, 1));
    }

    #[tokio::test]
    async fn a_take_that_cannot_be_saved_is_uncertain_and_still_shows_what_the_node_asked() {
        let taker = with_book(buy_book(), 0, false, 0).await;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(taker.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DROP TABLE orders")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let json = run(&taker, &args(&["--output", "json"], "take-buy", &["--yes"])).await;
        assert_eq!(json.code(), Some(1), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms_to_the_checked_in_schema("take-buy", &outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "order_not_saved");
        assert_eq!(outcome["data"]["payment_request"], HOLD_INVOICE);
        assert_eq!(outcome["meta"]["persistence"], "failed");
    }

    #[tokio::test]
    async fn a_human_take_that_cannot_be_saved_still_prints_the_hold_invoice_and_the_advice() {
        let taker = with_book(buy_book(), 0, false, 0).await;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(taker.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query("DROP TABLE orders")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let human = run(&taker, &args(&[], "take-buy", &["--yes"])).await;

        assert_eq!(human.code(), Some(1));
        assert!(human.stdout_text().lines().any(|line| line == HOLD_INVOICE));
        let err = human.stderr_text();
        assert!(
            err.contains("error: The node created the order but it could not be saved locally."),
            "{err}"
        );
        assert!(
            err.contains("Check `order info --order <id>` before trying again."),
            "{err}"
        );
        assert!(!err.contains(HOLD_INVOICE));
    }

    #[tokio::test]
    async fn the_amount_of_a_range_take_reaches_the_node_and_the_row() {
        let taker = with_book(
            vec![
                Listed::fixed(id(1), OrderKind::Sell, Keys::generate().public_key())
                    .ranged(100, 500),
            ],
            0,
            false,
            0,
        )
        .await;

        let run = run(
            &taker,
            &args(
                &["--output", "json"],
                "take-sell",
                &["--amount", "150", "--yes"],
            ),
        )
        .await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        assert_eq!(taker.fake.received()[0].amount, Some(150));
        assert_eq!(document(&run)["data"]["order"]["fiat_amount"], "150");
        assert_eq!(
            scalar::<i64>(&taker, "SELECT fiat_amount FROM orders").await,
            150
        );
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

    async fn tweak(taker: &Taker, how: Tweak) {
        let path = taker.sandbox.mcli_dir().join("mcli.db");
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
    fn ignores_read_only(taker: &Taker) -> bool {
        std::fs::OpenOptions::new()
            .write(true)
            .open(taker.sandbox.mcli_dir().join("mcli.db"))
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
        /// The failure comes after the trade was reserved (and given back):
        /// the orders and the raw index are compared instead of the bytes.
        reserved: bool,
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
                reserved: false,
            }
        }

        /// A failure inside the exchange, after the reservation.
        const fn after_reservation(label: &'static str, code: &'static str) -> Self {
            Self {
                reserved: true,
                ..Self::new(label, Tweak::None, code)
            }
        }
    }

    async fn run_env(
        taker: &Taker,
        args: &[&str],
        set: &[(&str, &str)],
        unset: &[&str],
    ) -> Captured {
        let mut command = command(taker, args);
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

    /// Run `case` as JSON, human and verbose, on both takes, and check what every
    /// mode shows, then that nothing was sent and nothing changed.
    async fn check_failure(case: &Failing, taker: &Taker) {
        tweak(taker, case.tweak).await;
        let before = snapshot(taker);
        let state_before = if case.reserved {
            Some(raw_state(taker).await)
        } else {
            None
        };
        let label = case.label;
        for command in ["take-sell", "take-buy"] {
            for mode in [&["--output", "json"][..], &[][..], &["--verbose"][..]] {
                let mut front: Vec<&str> = case.flags.to_vec();
                front.extend(mode);
                let run = run_env(
                    taker,
                    &args(&front, command, &["--yes"]),
                    case.set,
                    case.unset,
                )
                .await;
                assert_eq!(
                    run.code(),
                    Some(1),
                    "{label} {command} {mode:?}: {}",
                    run.stderr_text()
                );
                let (stdout, stderr) = (run.stdout_text(), run.stderr_text());
                for stream in [&stdout, &stderr] {
                    assert!(!stream.contains(SENTINEL), "{label}: a sentinel leaked");
                    assert!(
                        !stream.contains("127.0.0.1"),
                        "{label}: a relay address leaked"
                    );
                    assert!(
                        !stream.contains(HOLD_INVOICE),
                        "{label}: a hold invoice leaked"
                    );
                }
                if mode == ["--output", "json"] {
                    let outcome = document(&run);
                    conforms_to_the_checked_in_schema(command, &outcome);
                    assert_eq!(outcome["command"], format!("order.{command}"), "{label}");
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
                        !stderr.contains("Check `order info"),
                        "{label}: not uncertain"
                    );
                }
            }
        }
        assert!(
            taker.fake.received().is_empty(),
            "{label}: something was sent"
        );
        if let Some(state) = state_before {
            assert_eq!(raw_state(taker).await, state, "{label}: the store changed");
        } else if !matches!(case.tweak, Tweak::ReadOnly) {
            assert_eq!(snapshot(taker), before, "{label}: the store changed");
        }
        assert!(
            harness::is_empty_dir(&taker.sandbox.home())
                && harness::is_empty_dir(&taker.sandbox.cwd()),
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
            let taker = with_book(sell_book_and_buy(), 0, false, 0).await;
            check_failure(case, &taker).await;
            assert_eq!(
                taker.book.lock().unwrap().last_trade_index,
                0,
                "{}",
                case.label
            );
        }
    }

    /// One sell order and one buy order, both under the id the commands take.
    fn sell_book_and_buy() -> Vec<Listed> {
        let maker = Keys::generate().public_key();
        vec![
            Listed::fixed(id(1), OrderKind::Sell, maker),
            Listed::fixed(id(1), OrderKind::Buy, maker),
        ]
    }

    #[tokio::test]
    async fn a_store_that_cannot_be_written_stops_the_command_before_anything_is_sent() {
        let taker = with_book(sell_book(), 0, false, 0).await;
        tweak(&taker, Tweak::ReadOnly).await;
        if ignores_read_only(&taker) {
            return; // running as a user the file mode does not bind
        }
        let case = Failing::new("read-only store", Tweak::None, "store_unwritable");
        check_failure(&case, &taker).await;
    }

    #[tokio::test]
    async fn a_node_that_cannot_be_served_fails_before_publishing() {
        let (respond, book) = take_daemon(Book::default(), false);
        let old = taker_with(respond, book, 1, 0, false).await;
        check_failure(
            &Failing::after_reservation("protocol 1", "protocol_unsupported"),
            &old,
        )
        .await;

        let (respond, book) = take_daemon(Book::default(), false);
        let work = taker_with(respond, book, 2, 9, false).await;
        check_failure(
            &Failing::after_reservation("more work expected", "pow_required"),
            &work,
        )
        .await;

        // An explicit transport skips the protocol check and enough work passes.
        let (respond, book) = take_daemon(
            Book {
                orders: sell_book(),
                ..Book::default()
            },
            false,
        );
        let taker = taker_with(respond, book, 1, 9, false).await;
        let run = run(
            &taker,
            &args(
                &["--transport", "nip44", "--pow", "9", "--output", "json"],
                "take-sell",
                &["--yes"],
            ),
        )
        .await;
        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    }

    #[tokio::test]
    async fn a_relay_that_rejects_the_request_leaves_the_index_unspent() {
        let (respond, book) = take_daemon(Book::default(), false);
        let taker = taker_with(respond, book, 2, 0, true).await;
        check_failure(
            &Failing::after_reservation("rejected", "request_rejected"),
            &taker,
        )
        .await;
        assert_eq!(raw_state(&taker).await, (0, None));
    }

    #[tokio::test]
    async fn an_unreachable_relay_is_a_connection_failure_and_costs_nothing() {
        let taker = with_book(sell_book(), 0, false, 0).await;
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("ws://{}", free.local_addr().unwrap());
        drop(free);
        let before = raw_state(&taker).await;

        let run = run_env(
            &taker,
            &args(&["--output", "json"], "take-sell", &["--yes"]),
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
        assert_eq!(raw_state(&taker).await, before, "the index was given back");
    }

    #[tokio::test]
    async fn invalid_input_is_an_argument_error_before_any_effect() {
        let taker = with_book(sell_book(), 0, false, 0).await;
        let before = snapshot(&taker);
        let huge = "ln".to_string() + &"x".repeat(5000);
        let cases: Vec<(Vec<&str>, serde_json::Value)> = vec![
            (
                args(
                    &["--output", "json"],
                    "take-sell",
                    &["--amount", "0", "--yes"],
                ),
                json!("order.take-sell"),
            ),
            (
                args(
                    &["--output", "json"],
                    "take-sell",
                    &["--invoice", "SENTINEL zq", "--yes"],
                ),
                json!("order.take-sell"),
            ),
            (
                args(
                    &["--output", "json"],
                    "take-sell",
                    &["--invoice", &huge, "--yes"],
                ),
                json!("order.take-sell"),
            ),
            // Grammar errors keep the resolved command; a removed spelling has none.
            (
                args(
                    &["--output", "json"],
                    "take-buy",
                    &["--invoice", "lnbc1", "--yes"],
                ),
                json!("order.take-buy"),
            ),
            (
                args(
                    &["--output", "json"],
                    "take-sell",
                    &["--amount", "SENTINEL-zq81", "--yes"],
                ),
                json!("order.take-sell"),
            ),
            (
                args(
                    &["--output", "json"],
                    "take-sell",
                    &["--amount=-5", "--yes"],
                ),
                json!("order.take-sell"),
            ),
            (
                vec![
                    "--output",
                    "json",
                    "order",
                    "take-sell",
                    "--order",
                    "not-a-uuid",
                    "--yes",
                ],
                json!("order.take-sell"),
            ),
            (
                vec!["--output", "json", "order", "take-buy", "--yes"],
                json!("order.take-buy"),
            ),
            (
                vec!["--output", "json", "takesell", "--orderid", ORDER, "--yes"],
                serde_json::Value::Null,
            ),
            (
                vec!["--output", "json", "takebuy", "-o", ORDER, "--yes"],
                serde_json::Value::Null,
            ),
        ];
        for (arguments, command) in cases {
            let run = run(&taker, &arguments).await;
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
            assert!(!run.stdout_text().contains("zq"), "{arguments:?}");
            assert!(
                run.stderr.is_empty(),
                "{arguments:?}: {}",
                run.stderr_text()
            );
        }
        assert!(taker.fake.received().is_empty());
        assert_eq!(snapshot(&taker), before);
    }

    #[tokio::test]
    async fn an_unwritable_stdout_after_the_take_exits_1_with_one_fixed_line() {
        let taker = with_book(
            vec![Listed::fixed(
                id(1),
                OrderKind::Sell,
                Keys::generate().public_key(),
            )],
            0,
            false,
            0,
        )
        .await;
        // The first run takes the order; a second needs another one in the book.
        taker.book.lock().unwrap().orders.push(Listed::fixed(
            id(2),
            OrderKind::Sell,
            Keys::generate().public_key(),
        ));
        let identity_swap = |order: &str| -> Vec<String> {
            vec![
                "order".into(),
                "take-sell".into(),
                "--order".into(),
                order.into(),
                "--yes".into(),
            ]
        };
        for (front, order, line) in [
            (
                &["--output", "json"][..],
                ORDER,
                "error: the JSON outcome could not be written\n",
            ),
            (
                &[][..],
                "00000000-0000-0000-0000-000000000002",
                "error: the taken order could not be written\n",
            ),
        ] {
            let tail = identity_swap(order);
            let mut all: Vec<&str> = front.to_vec();
            all.extend(tail.iter().map(String::as_str));
            // One taker identity may hold only one pending order, so reset the book's memory.
            taker.book.lock().unwrap().takers.clear();
            let mut command = command(&taker, &all);
            command.stdout(harness::closed_pipe());
            let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
            assert_eq!(run.code(), Some(1), "{front:?}: {}", run.stderr_text());
            assert_eq!(run.stderr_text(), line, "{front:?}");
        }
        // Both takes happened and were saved all the same.
        assert_eq!(state(&taker).await, (2, 2));
    }

    #[tokio::test]
    async fn an_unwritable_stderr_changes_neither_the_exit_nor_the_document() {
        let taker = with_book(sell_book(), 0, false, 0).await;
        let mut command = command(
            &taker,
            &args(&["--output", "json", "--pow", "x"], "take-sell", &["--yes"]),
        );
        command.stderr(harness::closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1));
        assert_eq!(document(&run)["error"]["code"], "configuration_invalid");
    }
}
