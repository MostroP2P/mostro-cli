//! Tests for `order mine` (spec 011): the orders of the local store, read-only,
//! with no node, relay or Mostro configuration.
//!
//! The store is filled the way the commands fill it: `order new` and
//! `order take-sell` through the real binary against the scripted node, and a
//! range child adopted as `order sync` adopts it.

use std::path::{Path, PathBuf};

use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use nostr_sdk::prelude::{Keys, LocalRelay};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{
    info_event, new_order_daemon, seed_identity, set_last_trade_index, take_daemon, Book,
    FakeMostro, Listed, Respond,
};
use harness::{run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG};
use mostro_client::identity::trade_keys_from_mnemonic;
use mostro_client::store::{Store, Trade};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// A scripted node on its own loopback relay.
struct Node {
    relay: LoopbackRelay,
    fake: FakeMostro,
}

async fn node(respond: Respond) -> Node {
    let keys = Keys::generate();
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&keys, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let fake = FakeMostro::start(&relay.url, keys, respond).await;
    Node { relay, fake }
}

async fn run(sandbox: &Sandbox, args: &[&str], node: Option<&Node>) -> Captured {
    let mut command = sandbox.command(args);
    if let Some(node) = node {
        command
            .env("MOSTRO_PUBKEY", node.fake.pubkey().to_hex())
            .env("RELAYS", &node.relay.url);
    }
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
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

fn conforms(outcome: &Value) {
    let path = format!(
        "{}/docs/schemas/order-mine-v1.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(
        schema_check::conforms(&schema, &schema, outcome),
        "{outcome}"
    );
}

fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|entry| {
                    let path = entry.unwrap().path();
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    (path, bytes)
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

async fn pool(dir: &Path) -> SqlitePool {
    SqlitePool::connect_with(SqliteConnectOptions::new().filename(dir.join("mcli.db")))
        .await
        .unwrap()
}

async fn mnemonic(dir: &Path) -> String {
    let pool = pool(dir).await;
    let mnemonic = sqlx::query_scalar("SELECT mnemonic FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    mnemonic
}

async fn execute(dir: &Path, sql: &'static str) {
    let pool = pool(dir).await;
    sqlx::query(sql).execute(&pool).await.unwrap();
    pool.close().await;
}

fn pending_child(n: u128) -> SmallOrder {
    SmallOrder::new(
        Some(id(n)),
        Some(OrderKind::Sell),
        Some(Status::Pending),
        0,
        "USD".to_string(),
        Some(10),
        Some(60),
        0,
        "bank transfer".to_string(),
        1,
        None,
        None,
        None,
        Some(1_792_108_800),
        Some(1_792_195_200),
    )
}

/// An order made, an order taken and a range child adopted: all three are
/// listed with their index and key, newest first, with no relay configured.
#[tokio::test]
async fn lists_what_the_commands_saved_newest_first() {
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    let maker = node(new_order_daemon(0, false).0).await;
    let made = run(
        &sandbox,
        &[
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
        ],
        Some(&maker),
    )
    .await;
    assert_eq!(made.code(), Some(0), "{}", made.stdout_text());
    let made_id = document(&made)["data"]["order"]["id"].clone();
    let book = Book {
        orders: vec![Listed::fixed(
            id(1),
            OrderKind::Sell,
            Keys::generate().public_key(),
        )],
        last_trade_index: 0,
        ..Book::default()
    };
    let taker = node(take_daemon(book, false).0).await;
    let taken = run(
        &sandbox,
        &[
            "--output",
            "json",
            "order",
            "take-sell",
            "--order",
            &id(1).to_string(),
            "--yes",
        ],
        Some(&taker),
    )
    .await;
    assert_eq!(taken.code(), Some(0), "{}", taken.stdout_text());
    let words = mnemonic(&sandbox.mcli_dir()).await;
    let child_key = trade_keys_from_mnemonic(&words, 3).unwrap();
    let store = Store::in_dir(&sandbox.mcli_dir());
    let child = Trade {
        index: 3,
        keys: child_key.clone(),
    };
    assert!(store
        .adopt_order(&pending_child(5), &child, 7)
        .await
        .unwrap());
    let before = snapshot(&sandbox.mcli_dir());

    let json = run(
        &sandbox,
        &["--output", "json", "--verbose", "order", "mine"],
        None,
    )
    .await;
    let human = run(&sandbox, &["order", "mine"], None).await;

    assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
    assert_eq!(json.stderr_text(), "verbose: order.mine ok\n");
    let outcome = document(&json);
    conforms(&outcome);
    assert_eq!(outcome["command"], "order.mine");
    assert_eq!(
        outcome["meta"],
        json!({"publication": "not_applicable", "daemon_acceptance": "not_applicable",
               "persistence": "not_applicable", "snapshot": null})
    );
    let key = |index| {
        trade_keys_from_mnemonic(&words, index)
            .unwrap()
            .public_key()
            .to_hex()
    };
    assert_eq!(
        outcome["data"],
        json!({"orders": [
            {"id": id(5).to_string(), "side": "sell", "status": "pending",
             "trade_index": "3", "trade_pubkey": key(3)},
            {"id": id(1).to_string(), "side": "sell", "status": "waiting-buyer-invoice",
             "trade_index": "2", "trade_pubkey": key(2)},
            {"id": made_id, "side": "sell", "status": "pending",
             "trade_index": "1", "trade_pubkey": key(1)},
        ]})
    );
    assert_eq!(taker.fake.received()[0].author.to_hex(), key(2));
    assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
    let text = human.stdout_text();
    for order in outcome["data"]["orders"].as_array().unwrap() {
        assert!(text.contains(order["id"].as_str().unwrap()), "{text}");
    }
    assert_eq!(snapshot(&sandbox.mcli_dir()), before, "order mine wrote");
    for stream in [
        json.stdout_text(),
        json.stderr_text(),
        text,
        human.stderr_text(),
    ] {
        assert!(!stream.contains(&words), "the mnemonic leaked");
        assert!(
            !stream.contains(&child_key.secret_key().to_secret_hex()),
            "a trade key leaked"
        );
    }
}

#[tokio::test]
async fn an_empty_store_is_an_empty_list_and_needs_no_node() {
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;

    let json = run(
        &sandbox,
        &[
            "--mostro-pubkey",
            "garbage",
            "--output",
            "json",
            "order",
            "mine",
        ],
        None,
    )
    .await;
    let human = run(&sandbox, &["order", "mine"], None).await;

    assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
    let outcome = document(&json);
    conforms(&outcome);
    assert_eq!(outcome["data"], json!({"orders": []}));
    assert_eq!(outcome["warnings"], json!([]));
    assert!(
        human.stdout_text().contains("No local orders"),
        "{}",
        human.stdout_text()
    );
}

#[tokio::test]
async fn no_store_is_identity_missing_and_nothing_is_created() {
    let sandbox = Sandbox::new().unwrap();
    let json = run(&sandbox, &["--output", "json", "order", "mine"], None).await;
    let human = run(&sandbox, &["order", "mine"], None).await;

    assert_eq!(json.code(), Some(1));
    let outcome = document(&json);
    conforms(&outcome);
    assert_eq!(outcome["error"]["code"], "identity_missing");
    assert_eq!(outcome["data"], Value::Null);
    assert_eq!(human.code(), Some(1));
    assert_eq!(human.stderr_text(), "error: No local identity exists.\n");
    harness::assert_no_effects(&sandbox, "order mine without a store");
}

/// A file that is not a store, and a row whose status the program does not
/// know, are `store_unreadable`; nothing changes.
#[tokio::test]
async fn a_damaged_store_is_store_unreadable_and_left_alone() {
    for damage in ["garbage", "status"] {
        let sandbox = Sandbox::new().unwrap();
        seed_identity(&sandbox.mcli_dir()).await;
        let dir = sandbox.mcli_dir();
        if damage == "garbage" {
            std::fs::write(dir.join("mcli.db"), b"not a database at all, only text").unwrap();
        } else {
            execute(
                &dir,
                "INSERT INTO orders (id, kind, status, amount, fiat_code, fiat_amount, \
                 payment_method, premium) VALUES ('00000000-0000-0000-0000-000000000001', \
                 'sell', 'no-such-status', 0, 'USD', 1, 'x', 0)",
            )
            .await;
        }
        let before = snapshot(&dir);

        let run = run(&sandbox, &["--output", "json", "order", "mine"], None).await;

        assert_eq!(run.code(), Some(1), "{damage}");
        let outcome = document(&run);
        conforms(&outcome);
        assert_eq!(outcome["error"]["code"], "store_unreadable", "{damage}");
        assert_eq!(snapshot(&dir), before, "{damage}");
    }
}

/// A row without a key has neither key nor index; a key older than the newest
/// 200 derived ones has no index, and the run says so.
#[tokio::test]
async fn rows_whose_index_cannot_be_found_say_so() {
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    let dir = sandbox.mcli_dir();
    let words = mnemonic(&dir).await;
    let store = Store::in_dir(&dir);
    let old = Trade {
        index: 1,
        keys: trade_keys_from_mnemonic(&words, 1).unwrap(),
    };
    assert!(store.adopt_order(&pending_child(1), &old, 1).await.unwrap());
    assert!(store.adopt_order(&pending_child(2), &old, 1).await.unwrap());
    execute(
        &dir,
        "UPDATE orders SET trade_keys = NULL WHERE id = '00000000-0000-0000-0000-000000000002'",
    )
    .await;
    set_last_trade_index(&dir, 250).await;

    let run = run(&sandbox, &["--output", "json", "order", "mine"], None).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    let outcome = document(&run);
    conforms(&outcome);
    let orders = outcome["data"]["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 2);
    let by_id = |n: u128| {
        orders
            .iter()
            .find(|order| order["id"] == id(n).to_string())
            .unwrap()
            .clone()
    };
    assert_eq!(
        by_id(1)["trade_index"],
        Value::Null,
        "index 1 is older than the newest 200"
    );
    assert_eq!(by_id(1)["trade_pubkey"], old.keys.public_key().to_hex());
    assert_eq!(by_id(2)["trade_index"], Value::Null);
    assert_eq!(by_id(2)["trade_pubkey"], Value::Null);
    assert_eq!(outcome["warnings"][0]["code"], "trade_keys_limited");
}

#[tokio::test]
async fn options_are_argument_errors_before_any_effect() {
    let sandbox = Sandbox::new().unwrap();
    for args in [
        &["--output", "json", "order", "mine", "--yes"][..],
        &["--output", "json", "order", "mine", "--order", "x"],
    ] {
        let run = run(&sandbox, args, None).await;
        assert_eq!(run.code(), Some(2), "{args:?}");
        let outcome = document(&run);
        assert_eq!(outcome["command"], "order.mine");
        assert_eq!(outcome["error"]["code"], "invalid_arguments");
    }
    harness::assert_no_effects(&sandbox, "order mine options");
}
