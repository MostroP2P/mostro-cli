//! Tests for `order sync` (spec 008): the orders the node announced to the
//! user's trade keys that the store did not know get a local row.
//!
//! The mailbox is a loopback relay holding `new-order` messages the node
//! "sent" to the keys the store derives; the scripted node of the trade steps
//! answers on the same relay for the end-to-end case.

use std::time::Duration;

use chrono::Utc;
use mostro_core::order::{Kind as OrderKind, SmallOrder, Status};
use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::{Event, Keys, LocalRelay, Timestamp};
use serde_json::{json, Value};
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{info_event, node_event, seed_identity, set_last_trade_index};
use harness::{Backend, LoopbackRelay, Sandbox};
use mostro_client::commands::order::sync::{order_sync, SyncOutcome};
use mostro_client::msg::{PublicReadConfig, ReadLimits};
use mostro_client::outcome::sync::SyncInput;
use mostro_client::outcome::{ErrorCode, Status as Outcome};
use mostro_client::store::{Role, Store, Trade};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn minutes_ago(minutes: u64) -> Timestamp {
    Timestamp::from(Timestamp::now().as_secs() - minutes * 60)
}

/// A `new-order` announcement of `order` to the key of `index`, as the node
/// sends it: the order's id is the message id and the index rides along.
fn announcement(order: &SmallOrder, index: i64) -> Message {
    Message::new_order(
        order.id,
        Some(7777),
        Some(index),
        Action::NewOrder,
        Some(Payload::Order(order.clone())),
    )
}

/// A pending order of 10 to `max` fiat.
fn child(n: u128, max: i64) -> SmallOrder {
    SmallOrder::new(
        Some(id(n)),
        Some(OrderKind::Sell),
        Some(Status::Pending),
        0,
        "USD".to_string(),
        Some(10),
        Some(max),
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

struct Mailroom {
    sandbox: Sandbox,
    store: Store,
    identity: Keys,
    node: Keys,
    relay: LoopbackRelay,
    /// The keys of the user, newest first, as the command derives them.
    keys: Vec<(i64, Keys)>,
}

impl Mailroom {
    /// A user with `last` trades whose relay holds `events(node, keys)`.
    async fn with<F, Fut>(last: i64, events: F) -> Self
    where
        F: FnOnce(Keys, Vec<(i64, Keys)>) -> Fut,
        Fut: std::future::Future<Output = Vec<Event>>,
    {
        let sandbox = Sandbox::new().unwrap();
        let identity = seed_identity(&sandbox.mcli_dir()).await;
        set_last_trade_index(&sandbox.mcli_dir(), last).await;
        let store = Store::in_dir(&sandbox.mcli_dir());
        let keys = store.trade_keys(1000).await.unwrap().keys;
        let node = Keys::generate();
        let relay = LocalRelay::new();
        relay
            .add_event(info_event(&node, Some(2), Some(0), None, Some(10)))
            .await
            .unwrap();
        for event in events(node.clone(), keys.clone()).await {
            relay.add_event(event).await.unwrap();
        }
        let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
            .await
            .unwrap();
        Self {
            sandbox,
            store,
            identity,
            node,
            relay,
            keys,
        }
    }

    fn key(&self, index: i64) -> &Keys {
        &self
            .keys
            .iter()
            .find(|(i, _)| *i == index)
            .expect("a derived key")
            .1
    }

    fn config(&self) -> PublicReadConfig {
        PublicReadConfig::new(self.node.public_key(), &self.relay.url).unwrap()
    }

    fn database(&self) -> Vec<u8> {
        std::fs::read(self.sandbox.mcli_dir().join("mcli.db")).unwrap()
    }

    async fn sync(&self, since: Option<i64>) -> SyncOutcome {
        let input = SyncInput::validate(since).unwrap();
        order_sync(&input, &self.config(), &self.store, &limits(), Utc::now()).await
    }

    async fn query<T>(&self, sql: &'static str, order: Uuid) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(self.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        let value = sqlx::query_scalar(sql)
            .bind(order.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        value
    }

    async fn sql(&self, statement: &'static str) {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(self.sandbox.mcli_dir().join("mcli.db"));
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(statement).execute(&pool).await.unwrap();
        pool.close().await;
    }
}

fn limits() -> ReadLimits {
    ReadLimits {
        connect: Duration::from_secs(2),
        read: Duration::from_millis(1_500),
        cleanup: Duration::from_millis(500),
        ..ReadLimits::default()
    }
}

fn document(outcome: &SyncOutcome) -> Value {
    serde_json::to_value(outcome).unwrap()
}

fn adopted_ids(outcome: &SyncOutcome) -> Vec<String> {
    outcome
        .data()
        .expect("data")
        .adopted()
        .iter()
        .map(|adopted| adopted.order().id().to_string())
        .collect()
}

fn stages(outcome: &SyncOutcome) -> Value {
    let mut meta = document(outcome)["meta"].clone();
    meta.as_object_mut().unwrap().remove("snapshot");
    meta
}

fn meta(persistence: &str) -> Value {
    json!({"publication": "not_applicable", "daemon_acceptance": "not_applicable",
           "persistence": persistence})
}

/// One announcement of a child order to the key of index 3 (of three trades).
async fn one_child(node: Keys, keys: Vec<(i64, Keys)>) -> Vec<Event> {
    let to = keys.iter().find(|(i, _)| *i == 3).unwrap().1.public_key();
    vec![node_event(&node, to, &announcement(&child(5, 60), 3), minutes_ago(5)).await]
}

#[tokio::test]
async fn an_announced_order_the_store_does_not_know_gets_a_row_with_the_key_of_its_index() {
    let room = Mailroom::with(3, one_child).await;

    let outcome = room.sync(None).await;

    assert_eq!(outcome.status(), Outcome::Ok, "{:?}", document(&outcome));
    assert_eq!(adopted_ids(&outcome), [id(5).to_string()]);
    let adopted = &outcome.data().unwrap().adopted()[0];
    assert_eq!(adopted.trade_index(), "3");
    assert_eq!(adopted.trade_pubkey(), room.key(3).public_key().to_hex());
    assert_eq!(adopted.order().fiat_amount_min(), Some("10"));
    assert_eq!(adopted.order().fiat_amount_max(), Some("60"));
    assert_eq!(stages(&outcome), meta("applied"));
    // The row: the child's terms, the key of index 3 and the maker marker.
    assert_eq!(
        room.query::<String>("SELECT status FROM orders WHERE id = ?1", id(5))
            .await,
        "pending"
    );
    assert!(
        room.query::<bool>("SELECT is_mine FROM orders WHERE id = ?1", id(5))
            .await,
        "the user's own order"
    );
    assert_eq!(
        room.query::<i64>("SELECT max_amount FROM orders WHERE id = ?1", id(5))
            .await,
        60
    );
    let stored = room.store.order_keys(id(5)).await.unwrap().expect("a key");
    assert_eq!(stored.public_key(), room.key(3).public_key());
    assert_eq!(
        room.store
            .maker_range(id(5))
            .await
            .unwrap()
            .map(|r| r.sells),
        Some(true),
        "the child is a range order the user can continue"
    );
}

#[tokio::test]
async fn a_second_run_saves_nothing_and_changes_no_byte() {
    let room = Mailroom::with(3, one_child).await;
    room.sync(None).await;
    let before = room.database();

    let again = room.sync(None).await;

    assert_eq!(again.status(), Outcome::Ok);
    assert!(adopted_ids(&again).is_empty());
    assert_eq!(stages(&again), meta("not_attempted"));
    assert_eq!(again.data().unwrap().messages_read(), "1");
    assert_eq!(again.data().unwrap().messages_ignored(), "1");
    assert_eq!(room.database(), before, "idempotent");
}

#[tokio::test]
async fn a_known_order_is_never_replaced() {
    let room = Mailroom::with(3, one_child).await;
    let trade = Trade {
        index: 3,
        keys: room.key(3).clone(),
    };
    let mut known = child(5, 60);
    known.status = Some(Status::Active);
    room.store
        .save_order_as(&known, &trade, 1, Role::Taker)
        .await
        .unwrap();
    let before = room.database();

    let outcome = room.sync(None).await;

    assert!(adopted_ids(&outcome).is_empty());
    assert_eq!(room.database(), before, "the row stays as it was");
    assert_eq!(
        room.query::<String>("SELECT status FROM orders WHERE id = ?1", id(5))
            .await,
        "active"
    );
}

#[tokio::test]
async fn the_newest_announcement_wins_and_several_orders_come_oldest_first() {
    let room = Mailroom::with(3, |node, keys| async move {
        let to = |i: i64| keys.iter().find(|(n, _)| *n == i).unwrap().1.public_key();
        let mut later = child(5, 60);
        later.status = Some(Status::WaitingTakerBond);
        vec![
            node_event(
                &node,
                to(2),
                &announcement(&child(6, 40), 2),
                minutes_ago(30),
            )
            .await,
            node_event(
                &node,
                to(3),
                &announcement(&child(5, 60), 3),
                minutes_ago(20),
            )
            .await,
            node_event(&node, to(3), &announcement(&later, 3), minutes_ago(10)).await,
        ]
    })
    .await;

    let outcome = room.sync(None).await;

    assert_eq!(
        adopted_ids(&outcome),
        [id(6).to_string(), id(5).to_string()]
    );
    assert_eq!(
        room.query::<String>("SELECT status FROM orders WHERE id = ?1", id(5))
            .await,
        "waiting-taker-bond",
        "the newest announcement"
    );
    assert_eq!(outcome.data().unwrap().messages_read(), "3");
    assert_eq!(outcome.data().unwrap().messages_ignored(), "1");
}

#[tokio::test]
async fn only_announcements_the_node_signed_to_one_of_the_users_keys_in_the_window_count() {
    let room = Mailroom::with(3, |node, keys| async move {
        let to = |i: i64| keys.iter().find(|(n, _)| *n == i).unwrap().1.public_key();
        let stranger = Keys::generate();
        let mismatched = Message::new_order(
            Some(id(98)),
            None,
            Some(2),
            Action::NewOrder,
            Some(Payload::Order(child(99, 40))),
        );
        vec![
            // Other actions are not announcements.
            node_event(
                &node,
                to(1),
                &Message::new_order(
                    Some(id(10)),
                    None,
                    Some(1),
                    Action::PayInvoice,
                    Some(Payload::Order(child(10, 40))),
                ),
                minutes_ago(5),
            )
            .await,
            // Signed by someone else.
            node_event(
                &stranger,
                to(2),
                &announcement(&child(11, 40), 2),
                minutes_ago(5),
            )
            .await,
            // Addressed to a key the user does not have.
            node_event(
                &node,
                Keys::generate().public_key(),
                &announcement(&child(12, 40), 4),
                minutes_ago(5),
            )
            .await,
            // The message id and the order's id disagree.
            node_event(&node, to(2), &mismatched, minutes_ago(5)).await,
            // No order in the payload.
            node_event(
                &node,
                to(2),
                &Message::new_order(Some(id(13)), None, Some(2), Action::NewOrder, None),
                minutes_ago(5),
            )
            .await,
            // Too old for the window.
            node_event(
                &node,
                to(3),
                &announcement(&child(14, 40), 3),
                minutes_ago(120),
            )
            .await,
            // The only valid one.
            node_event(
                &node,
                to(1),
                &announcement(&child(15, 40), 1),
                minutes_ago(5),
            )
            .await,
        ]
    })
    .await;

    let outcome = room.sync(Some(60)).await;

    assert_eq!(outcome.status(), Outcome::Ok, "{:?}", document(&outcome));
    assert_eq!(adopted_ids(&outcome), [id(15).to_string()]);
}

#[tokio::test]
async fn nothing_to_add_is_a_success_that_writes_nothing() {
    let room = Mailroom::with(3, |_, _| async { Vec::new() }).await;
    let before = room.database();

    let outcome = room.sync(None).await;

    assert_eq!(outcome.status(), Outcome::Ok);
    assert_eq!(stages(&outcome), meta("not_attempted"));
    assert_eq!(outcome.data().unwrap().messages_read(), "0");
    assert_eq!(outcome.data().unwrap().keys_read(), "3");
    assert_eq!(room.database(), before);
}

#[tokio::test]
async fn a_save_that_fails_stops_the_run_and_keeps_the_rows_already_saved() {
    let room = Mailroom::with(3, |node, keys| async move {
        let to = |i: i64| keys.iter().find(|(n, _)| *n == i).unwrap().1.public_key();
        vec![
            node_event(
                &node,
                to(2),
                &announcement(&child(6, 40), 2),
                minutes_ago(30),
            )
            .await,
            node_event(
                &node,
                to(3),
                &announcement(&child(5, 60), 3),
                minutes_ago(20),
            )
            .await,
        ]
    })
    .await;
    room.sql(
        "CREATE TRIGGER refuse BEFORE INSERT ON orders \
         WHEN new.id = '00000000-0000-0000-0000-000000000005' \
         BEGIN SELECT RAISE(ABORT, 'refused'); END",
    )
    .await;

    let outcome = room.sync(None).await;

    assert_eq!(outcome.status(), Outcome::Uncertain);
    assert_eq!(outcome.error_code(), Some(ErrorCode::OrderNotSaved));
    assert_eq!(adopted_ids(&outcome), [id(6).to_string()], "what was saved");
    assert_eq!(stages(&outcome), meta("failed"));
    assert!(room.store.order_keys(id(6)).await.unwrap().is_some());
    assert!(room.store.order_keys(id(5)).await.unwrap().is_none());
}

#[tokio::test]
async fn failures_before_a_row_is_written_change_nothing() {
    // No store at all.
    let room = Mailroom::with(3, one_child).await;
    std::fs::remove_file(room.sandbox.mcli_dir().join("mcli.db")).unwrap();
    let outcome = room.sync(None).await;
    assert_eq!(outcome.error_code(), Some(ErrorCode::IdentityMissing));
    assert!(!room.sandbox.mcli_dir().join("mcli.db").exists());

    // An unreachable relay.
    let room = Mailroom::with(3, one_child).await;
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("ws://{}", free.local_addr().unwrap());
    drop(free);
    let before = room.database();
    let config = PublicReadConfig::new(room.node.public_key(), &dead).unwrap();
    let input = SyncInput::validate(None).unwrap();
    let outcome = order_sync(&input, &config, &room.store, &limits(), Utc::now()).await;
    assert_eq!(outcome.status(), Outcome::Error);
    assert_eq!(outcome.error_code(), Some(ErrorCode::RelayConnectionFailed));
    assert_eq!(stages(&outcome), meta("not_attempted"));
    assert_eq!(room.database(), before);
}

#[tokio::test]
async fn an_adopted_child_can_be_acted_on_at_once() {
    use fake_mostro::{step_daemon, FakeMostro, Traded};
    use mostro_client::commands::order::step::order_step;
    use mostro_client::commands::order::WriteOptions;
    use mostro_client::msg::ExchangeLimits;
    use mostro_client::outcome::step::{StepInput, StepKind, StepResult};

    let room = Mailroom::with(3, one_child).await;
    room.sync(None).await;
    // The scripted node knows the child as a pending order of that key.
    let (respond, _) = step_daemon(vec![Traded::pending(id(5), room.key(3).public_key())]);
    let fake = FakeMostro::start(&room.relay.url, room.node.clone(), respond).await;

    let outcome = order_step(
        &StepInput::new(StepKind::Cancel, id(5)),
        &room.config(),
        &room.identity,
        &room.store,
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
    .await;

    assert_eq!(
        outcome.status(),
        Outcome::Ok,
        "{:?}",
        serde_json::to_value(&outcome)
    );
    assert_eq!(outcome.data().unwrap().result(), StepResult::Canceled);
    assert_eq!(fake.received().len(), 1);
    assert_eq!(fake.received()[0].author, room.key(3).public_key());
}

// --- The binary ----------------------------------------------------------------

mod binary {
    use super::*;
    use harness::{closed_pipe, is_empty_dir, run_with_watchdog, Captured, WATCHDOG};

    fn command(room: &Mailroom, args: &[&str]) -> tokio::process::Command {
        let mut command = room.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", room.node.public_key().to_hex())
            .env("RELAYS", &room.relay.url);
        command
    }

    async fn run(room: &Mailroom, args: &[&str]) -> Captured {
        let run = run_with_watchdog(command(room, args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    async fn run_env(
        room: &Mailroom,
        args: &[&str],
        set: &[(&str, &str)],
        unset: &[&str],
    ) -> Captured {
        let mut command = command(room, args);
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

    fn outcome(run: &Captured) -> Value {
        let stdout = run.stdout_text();
        assert!(
            stdout.ends_with('\n') && stdout.lines().count() == 1,
            "{stdout}"
        );
        serde_json::from_str(&stdout).unwrap()
    }

    fn conforms_to_the_checked_in_schema(outcome: &Value) {
        let path = format!(
            "{}/docs/schemas/order-sync-v1.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(
            schema_check::conforms(&schema, &schema, outcome),
            "{outcome}"
        );
    }

    fn snapshot(room: &Mailroom) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(room.sandbox.mcli_dir())
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

    #[tokio::test]
    async fn json_saves_the_order_and_a_second_run_saves_nothing() {
        let room = Mailroom::with(3, one_child).await;

        let first = run(&room, &["--output", "json", "--verbose", "order", "sync"]).await;

        assert_eq!(first.code(), Some(0), "{}", first.stderr_text());
        assert_eq!(first.stderr_text(), "verbose: order.sync ok\n");
        let document = outcome(&first);
        conforms_to_the_checked_in_schema(&document);
        assert_eq!(document["command"], "order.sync");
        assert_eq!(document["status"], "ok");
        assert_eq!(
            document["data"]["adopted"][0]["order"]["id"],
            id(5).to_string()
        );
        assert_eq!(document["data"]["adopted"][0]["trade_index"], "3");
        assert_eq!(document["meta"]["persistence"], "applied");
        assert_eq!(document["meta"]["publication"], "not_applicable");
        let secret = room.key(3).secret_key().to_secret_hex();
        for stream in [first.stdout_text(), first.stderr_text()] {
            assert!(!stream.contains(&secret), "the trade key leaked");
            assert!(!stream.contains("127.0.0.1"), "a relay address leaked");
        }

        let before = snapshot(&room);
        let second = run(&room, &["--output", "json", "order", "sync"]).await;
        let document = outcome(&second);
        conforms_to_the_checked_in_schema(&document);
        assert_eq!(document["data"]["adopted"], json!([]));
        assert_eq!(document["meta"]["persistence"], "not_attempted");
        assert_eq!(snapshot(&room), before);
    }

    #[tokio::test]
    async fn human_output_lists_what_was_added_and_what_to_do() {
        let room = Mailroom::with(3, one_child).await;

        let run = run(&room, &["order", "sync", "--since", "60"]).await;

        assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
        let out = run.stdout_text();
        assert!(
            out.contains("Order Sync") && out.contains("1 order(s) added"),
            "{out}"
        );
        assert!(
            out.contains(&id(5).to_string()) && out.contains("Trade Index: 3"),
            "{out}"
        );
        assert!(out.contains("order cancel --order"), "{out}");
        assert!(run.stderr.is_empty(), "{}", run.stderr_text());
        let again = self::run(&room, &["order", "sync"]).await;
        assert!(again.stdout_text().contains("Nothing to add"));
    }

    #[tokio::test]
    async fn an_invalid_window_or_an_unknown_option_is_an_argument_error_before_any_effect() {
        let room = Mailroom::with(3, one_child).await;
        let before = snapshot(&room);

        for args in [
            vec!["--output", "json", "order", "sync", "--since", "0"],
            vec!["--output", "json", "order", "sync", "--since", "10081"],
            vec!["--output", "json", "order", "sync", "--since", "x"],
            vec!["--output", "json", "order", "sync", "--yes"],
        ] {
            let run = run(&room, &args).await;
            assert_eq!(run.code(), Some(2), "{args:?}: {}", run.stderr_text());
            let document = outcome(&run);
            assert_eq!(document["error"]["code"], "invalid_arguments", "{args:?}");
            assert!(run.stderr.is_empty(), "{args:?}");
        }
        assert_eq!(snapshot(&room), before);
    }

    #[tokio::test]
    async fn failures_before_a_row_is_written_are_stable_and_leave_no_trace() {
        type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [&'a str], &'a str);
        let cases: [Case; 4] = [
            (
                "no node key",
                &[],
                &["MOSTRO_PUBKEY"],
                "configuration_missing",
            ),
            ("no relays", &[], &["RELAYS"], "configuration_missing"),
            (
                "a bad node key",
                &[("MOSTRO_PUBKEY", "SENTINEL-zq81")],
                &[],
                "configuration_invalid",
            ),
            (
                "a bad relay",
                &[("RELAYS", "http://SENTINEL-zq81.invalid")],
                &[],
                "configuration_invalid",
            ),
        ];
        for (label, set, unset, code) in cases {
            let room = Mailroom::with(3, one_child).await;
            let before = snapshot(&room);
            for mode in [&["--output", "json"][..], &[][..]] {
                let mut args = mode.to_vec();
                args.extend(["order", "sync"]);
                let run = run_env(&room, &args, set, unset).await;
                assert_eq!(run.code(), Some(1), "{label}: {}", run.stderr_text());
                for stream in [run.stdout_text(), run.stderr_text()] {
                    assert!(!stream.contains("SENTINEL"), "{label}");
                    assert!(!stream.contains("127.0.0.1"), "{label}");
                }
                if mode.is_empty() {
                    assert!(run.stdout.is_empty() && run.stderr_text().starts_with("error: "));
                } else {
                    let document = outcome(&run);
                    conforms_to_the_checked_in_schema(&document);
                    assert_eq!(document["error"]["code"], code, "{label}");
                    assert!(run.stderr.is_empty());
                }
            }
            assert_eq!(snapshot(&room), before, "{label}");
            assert!(is_empty_dir(&room.sandbox.home()) && is_empty_dir(&room.sandbox.cwd()));
        }
    }

    #[tokio::test]
    async fn a_missing_store_and_a_read_only_store_stop_before_the_relay() {
        let room = Mailroom::with(3, one_child).await;
        let path = room.sandbox.mcli_dir().join("mcli.db");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let writable = std::fs::OpenOptions::new().write(true).open(&path).is_ok();
        if !writable {
            let run = run(&room, &["--output", "json", "order", "sync"]).await;
            assert_eq!(run.code(), Some(1));
            assert_eq!(outcome(&run)["error"]["code"], "store_unwritable");
        }
        let room = Mailroom::with(3, one_child).await;
        std::fs::remove_file(room.sandbox.mcli_dir().join("mcli.db")).unwrap();
        let run = run(&room, &["--output", "json", "order", "sync"]).await;
        assert_eq!(run.code(), Some(1));
        assert_eq!(outcome(&run)["error"]["code"], "identity_missing");
        assert!(!room.sandbox.mcli_dir().join("mcli.db").exists());
    }

    #[tokio::test]
    async fn an_unwritable_stdout_exits_1_with_one_fixed_line_after_the_row_was_saved() {
        for (front, line) in [
            (
                &["--output", "json"][..],
                "error: the JSON outcome could not be written\n",
            ),
            (&[][..], "error: the sync result could not be written\n"),
        ] {
            let room = Mailroom::with(3, one_child).await;
            let mut args = front.to_vec();
            args.extend(["order", "sync"]);
            let mut command = command(&room, &args);
            command.stdout(closed_pipe());
            let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
            assert_eq!(run.code(), Some(1), "{front:?}: {}", run.stderr_text());
            assert_eq!(run.stderr_text(), line);
            assert!(room.store.order_keys(id(5)).await.unwrap().is_some());
        }
        let room = Mailroom::with(3, one_child).await;
        let mut command = command(&room, &["--output", "json", "order", "sync"]);
        command
            .env("RELAYS", "http://SENTINEL.invalid")
            .stderr(closed_pipe());
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert_eq!(run.code(), Some(1));
        assert_eq!(outcome(&run)["error"]["code"], "configuration_invalid");
    }
}
