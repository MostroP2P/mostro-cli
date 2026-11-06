//! Tests for the bond flows of the migrated commands (spec 015).
//!
//! (a) `order cancel` after a bonded `order take-sell` (the take waits on the
//! taker bond) and after a bonded `order new` (the order waits on the maker
//! bond), through the real binary: the take or the order is answered by the
//! scripted node's bonded scripts, the cancel by its step script set to the
//! bond status the source parks the order at.
//! (b) `message list --order` over the bond messages a node volunteers.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::Action;
use nostr_sdk::prelude::{Keys, LocalRelay, Timestamp};
use serde_json::Value;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{
    info_event, mail_bond_notice, mail_bond_payout_request, new_order_daemon, node_event,
    seed_identity, step_daemon, take_daemon, Book, FakeMostro, Listed, Respond, Traded,
};
use harness::{run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG};
use mostro_client::store::{Store, Trade};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// The bonded script for the first request and the step script for `cancel`.
fn bonded(first: Respond, steps: Respond) -> Respond {
    Arc::new(move |request| match request.action {
        Action::Cancel => steps(request),
        _ => first(request),
    })
}

struct World {
    relay: LoopbackRelay,
    fake: FakeMostro,
    trades: Arc<Mutex<Vec<Traded>>>,
    sandbox: Sandbox,
}

async fn world(first: Respond) -> World {
    let (steps, trades) = step_daemon(Vec::new());
    let node = Keys::generate();
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&node, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, node, bonded(first, steps)).await;
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    World {
        relay,
        fake,
        trades,
        sandbox,
    }
}

impl World {
    async fn run(&self, args: &[&str]) -> Captured {
        let mut command = self.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", self.fake.pubkey().to_hex())
            .env("RELAYS", &self.relay.url);
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    async fn local_status(&self, order: &str) -> String {
        let options = SqliteConnectOptions::new()
            .filename(self.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        let status = sqlx::query_scalar("SELECT status FROM orders WHERE id = ?1")
            .bind(order)
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        status
    }
}

fn document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert!(stdout.lines().count() == 1, "{stdout}");
    serde_json::from_str(&stdout).unwrap()
}

/// A take parked on its bond: the row and key exist, and the taker backs out
/// with `order cancel`; the node puts the order back on the book.
#[tokio::test]
async fn a_taker_cancels_a_take_that_waits_on_its_bond() {
    let maker = Keys::generate().public_key();
    let book = Book {
        orders: vec![Listed::fixed(id(1), OrderKind::Sell, maker)],
        last_trade_index: 0,
        ..Book::default()
    };
    let world = world(take_daemon(book, true).0).await;
    let order = id(1).to_string();

    let take = world
        .run(&[
            "--output",
            "json",
            "order",
            "take-sell",
            "--order",
            &order,
            "--yes",
        ])
        .await;

    assert_eq!(take.code(), Some(0), "{}", take.stdout_text());
    assert_eq!(document(&take)["data"]["next_step"], "pay_bond_invoice");
    let taker = world.fake.received()[0].author;
    let store = Store::in_dir(&world.sandbox.mcli_dir());
    let key = store
        .order_keys(id(1))
        .await
        .unwrap()
        .expect("the row has its key");
    assert_eq!(key.public_key(), taker);
    assert_eq!(
        world.local_status(&order).await,
        "pending",
        "the order as the node sent it"
    );
    world.trades.lock().unwrap().push(Traded {
        buyer: Some(taker),
        ..Traded::pending(id(1), maker).with_status(Status::WaitingTakerBond)
    });
    world.trades.lock().unwrap()[0].seller = Some(maker);

    let cancel = world
        .run(&[
            "--output", "json", "order", "cancel", "--order", &order, "--yes",
        ])
        .await;

    assert_eq!(cancel.code(), Some(0), "{}", cancel.stdout_text());
    let outcome = document(&cancel);
    assert_eq!(outcome["data"]["result"], "canceled");
    assert_eq!(outcome["meta"]["persistence"], "applied");
    assert_eq!(world.local_status(&order).await, "canceled");
    assert_eq!(
        world.fake.received()[1].author,
        taker,
        "signed by the take's key"
    );
    let trades = world.trades.lock().unwrap();
    assert_eq!(
        trades[0].status,
        Status::Pending,
        "back on the book for others"
    );
}

/// An order parked on the maker bond, never published: the maker cancels it.
#[tokio::test]
async fn a_maker_cancels_an_order_that_waits_on_its_bond() {
    let world = world(new_order_daemon(0, true).0).await;

    let made = world
        .run(&[
            "--output",
            "json",
            "order",
            "new",
            "--side",
            "sell",
            "--currency",
            "USD",
            "--fiat-amount",
            "10",
            "--payment-method",
            "bank transfer",
            "--yes",
        ])
        .await;

    assert_eq!(made.code(), Some(0), "{}", made.stdout_text());
    let outcome = document(&made);
    assert_eq!(outcome["data"]["next_step"], "pay_bond_invoice");
    let order = outcome["data"]["order"]["id"].as_str().unwrap().to_string();
    let maker = world.fake.received()[0].author;
    let id = Uuid::parse_str(&order).unwrap();
    world
        .trades
        .lock()
        .unwrap()
        .push(Traded::pending(id, maker).with_status(Status::WaitingMakerBond));

    let cancel = world
        .run(&[
            "--output", "json", "order", "cancel", "--order", &order, "--yes",
        ])
        .await;

    assert_eq!(cancel.code(), Some(0), "{}", cancel.stdout_text());
    assert_eq!(document(&cancel)["data"]["result"], "canceled");
    assert_eq!(world.local_status(&order).await, "canceled");
    assert_eq!(world.trades.lock().unwrap()[0].status, Status::Canceled);
}

/// Someone who is not the maker cannot close an order that waits on the maker
/// bond: the refusal carries the node's reason.
#[tokio::test]
async fn only_the_maker_closes_an_order_that_waits_on_its_bond() {
    let world = world(new_order_daemon(0, true).0).await;
    let store = Store::in_dir(&world.sandbox.mcli_dir());
    let stranger = Keys::generate();
    let order = SmallOrder {
        id: Some(id(7)),
        kind: Some(OrderKind::Sell),
        status: Some(Status::Pending),
        fiat_code: "USD".to_string(),
        payment_method: "x".to_string(),
        fiat_amount: 10,
        ..SmallOrder::default()
    };
    let trade = Trade {
        index: 1,
        keys: stranger.clone(),
    };
    store.save_order(&order, &trade, 1).await.unwrap();
    world.trades.lock().unwrap().push(
        Traded::pending(id(7), Keys::generate().public_key()).with_status(Status::WaitingMakerBond),
    );

    let cancel = world
        .run(&[
            "--output",
            "json",
            "order",
            "cancel",
            "--order",
            &id(7).to_string(),
            "--yes",
        ])
        .await;

    assert_eq!(cancel.code(), Some(1));
    let outcome = document(&cancel);
    assert_eq!(outcome["error"]["code"], "node_refused");
    assert_eq!(outcome["error"]["reason"], "is_not_your_order");
}

/// The four bond messages through `message list --order`: the payout request
/// with its next step and share, and the three notices with an order payload.
#[tokio::test]
async fn message_list_reports_the_bond_messages() {
    let node = Keys::generate();
    let trade = Keys::generate();
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    let row = SmallOrder {
        id: Some(id(9)),
        kind: Some(OrderKind::Sell),
        status: Some(Status::Success),
        amount: 6_000,
        fiat_code: "USD".to_string(),
        fiat_amount: 5,
        payment_method: "bank".to_string(),
        ..SmallOrder::default()
    };
    Store::in_dir(&sandbox.mcli_dir())
        .save_order(
            &row,
            &Trade {
                index: 1,
                keys: trade.clone(),
            },
            1,
        )
        .await
        .unwrap();
    let share = SmallOrder {
        amount: 1_500,
        ..row.clone()
    };
    let slashed = SmallOrder {
        status: None,
        amount: 3_000,
        ..row.clone()
    };
    let now = Timestamp::now().as_secs();
    let at = |minutes: u64| Timestamp::from(now - minutes * 60);
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&node, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let to = trade.public_key();
    for (message, minutes) in [
        (mail_bond_notice(Action::BondSlashed, &slashed), 40),
        (mail_bond_payout_request(&share, 1_792_000_000), 30),
        (mail_bond_notice(Action::BondInvoiceAccepted, &share), 20),
        (mail_bond_notice(Action::BondPayoutCompleted, &share), 10),
    ] {
        relay
            .add_event(node_event(&node, to, &message, at(minutes)).await)
            .await
            .unwrap();
    }
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let mut command = sandbox.command([
        "--output",
        "json",
        "message",
        "list",
        "--order",
        &id(9).to_string(),
        "--since",
        "60",
    ]);
    command
        .env("MOSTRO_PUBKEY", node.public_key().to_hex())
        .env("RELAYS", &relay.url);

    let run = run_with_watchdog(command, Duration::from_secs(40))
        .await
        .unwrap();

    assert_eq!(
        run.code(),
        Some(0),
        "{} {}",
        run.stdout_text(),
        run.stderr_text()
    );
    let outcome = document(&run);
    let messages = outcome["data"]["messages"].as_array().unwrap();
    let actions: Vec<&str> = messages
        .iter()
        .map(|m| m["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        [
            "bond-slashed",
            "add-bond-invoice",
            "bond-invoice-accepted",
            "bond-payout-completed"
        ]
    );
    let request = &messages[1];
    assert_eq!(request["next_step"], "add_bond_invoice");
    assert_eq!(request["payload"]["kind"], "bond_payout_request");
    assert_eq!(
        request["payload"]["order"]["sats_amount"], "1500",
        "the share"
    );
    assert_eq!(request["payload"]["slashed_at_unix"], "1792000000");
    for (index, amount) in [(0, "3000"), (2, "1500"), (3, "1500")] {
        let notice = &messages[index];
        assert_eq!(notice["next_step"], Value::Null, "{index}");
        assert_eq!(notice["payload"]["kind"], "order", "{index}");
        assert_eq!(
            notice["payload"]["order"]["id"],
            id(9).to_string(),
            "{index}"
        );
        assert_eq!(notice["payload"]["order"]["sats_amount"], amount, "{index}");
    }
    assert_eq!(messages[0]["payload"]["order"]["status"], Value::Null);
}
