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
use nostr_sdk::prelude::{Client, ClientNotification, Filter, Keys, Kind, LocalRelay, PublicKey};
use tokio::time::timeout;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{info_event, take_daemon, Book, FakeMostro, Listed, Respond, HOLD_INVOICE};
use harness::{Backend, LoopbackRelay};

const REQUEST_ID: u64 = 6150;
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
        _relay: LoopbackRelay,
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
        Taker {
            _relay: relay,
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
        let trade = taker.store.next_trade().await.unwrap();
        order_take(
            input,
            &taker.config,
            &taker.identity,
            &trade,
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
    async fn a_bonded_node_is_an_error_that_spends_the_index() {
        let taker = with_book(sell_book(), 0, true, 0).await;

        let outcome = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;

        assert_eq!(outcome.status(), Outcome::Error);
        assert_eq!(outcome.error_code(), Some(ErrorCode::BondRequired));
        assert_eq!(stages(&outcome), accepted_and_applied());
        assert_eq!(state(&taker).await, (0, 1));
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
        let before = std::fs::read(taker.sandbox.mcli_dir().join("mcli.db")).unwrap();

        let outcome = take(&taker, &input(TakeSide::Sell, id(1), None, None)).await;

        assert_eq!(outcome.status(), Outcome::Error);
        assert_eq!(outcome.error_code(), Some(ErrorCode::PowRequired));
        assert_eq!(
            stages(&outcome),
            json!({"publication": "not_attempted", "daemon_acceptance": "unknown",
                   "persistence": "not_attempted", "snapshot": null})
        );
        assert!(taker.fake.received().is_empty());
        assert_eq!(state(&taker).await, (0, 0));
        assert_eq!(
            std::fs::read(taker.sandbox.mcli_dir().join("mcli.db")).unwrap(),
            before
        );
    }
}
