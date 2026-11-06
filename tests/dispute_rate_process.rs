//! Tests for `dispute open` (spec 013) and `order rate` (spec 014) against the
//! scripted node, which applies the source's `dispute_action` and
//! `update_user_reputation_action` rules.
//!
//! The user holds order 1 under one trade key; the scripted node knows the
//! trade with that key as the buyer and another key as the seller.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mostro_core::order::Status;
use mostro_core::prelude::{Action, CantDoReason, Message, Payload};
use nostr_sdk::prelude::{Keys, LocalRelay, PublicKey};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{query, query_scalar, SqlitePool};
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{
    dispute_id, info_event, seed_identity, step_daemon, FakeMostro, Outgoing, Owed, Respond, Traded,
};
use harness::{
    run_with_input, run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG,
};
use mostro_client::store::{Store, Trade};

const ORDER: &str = "00000000-0000-0000-0000-000000000001";
/// Every update of the local orders fails.
const FREEZE: &str = "CREATE TRIGGER freeze BEFORE UPDATE ON orders \
                      BEGIN SELECT RAISE(ABORT, 'frozen'); END";

fn order_id() -> Uuid {
    Uuid::from_u128(1)
}

struct Holder {
    relay: LoopbackRelay,
    fake: FakeMostro,
    trades: Arc<Mutex<Vec<Traded>>>,
    sandbox: Sandbox,
    buyer: Keys,
}

/// The user is the buyer of order 1, which the scripted node knows in `status`.
async fn holder(status: Status) -> Holder {
    holder_with(status, |respond| respond).await
}

/// As [`holder`], with the node's answers passed through `wrap`.
async fn holder_with(status: Status, wrap: impl FnOnce(Respond) -> Respond) -> Holder {
    let buyer = Keys::generate();
    let seller = Keys::generate().public_key();
    let trade = Traded::active(order_id(), seller, buyer.public_key()).with_status(status);
    let (respond, trades) = step_daemon(vec![trade]);
    let respond = wrap(respond);
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
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    let store = Store::in_dir(&sandbox.mcli_dir());
    let small = Owed::waiting(order_id(), buyer.public_key()).small(Status::Active);
    let row = Trade {
        index: 1,
        keys: buyer.clone(),
    };
    store.save_order(&small, &row, 1).await.unwrap();
    Holder {
        relay,
        fake,
        trades,
        sandbox,
        buyer,
    }
}

impl Holder {
    fn command(&self, args: &[&str]) -> tokio::process::Command {
        let mut command = self.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", self.fake.pubkey().to_hex())
            .env("RELAYS", &self.relay.url);
        command
    }

    async fn run(&self, args: &[&str]) -> Captured {
        let run = run_with_watchdog(self.command(args), WATCHDOG)
            .await
            .unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        run
    }

    async fn answering(&self, args: &[&str], answer: &str) -> Captured {
        run_with_input(self.command(args), answer.as_bytes().to_vec(), WATCHDOG)
            .await
            .unwrap()
    }

    async fn status(&self) -> String {
        let options = SqliteConnectOptions::new()
            .filename(self.sandbox.mcli_dir().join("mcli.db"))
            .read_only(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        let status = query_scalar("SELECT status FROM orders WHERE id = ?1")
            .bind(ORDER)
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        status
    }

    /// Run `statement` on the store, outside the command under test.
    async fn sql(&self, statement: &'static str) {
        let options = SqliteConnectOptions::new().filename(self.sandbox.mcli_dir().join("mcli.db"));
        let pool = SqlitePool::connect_with(options).await.unwrap();
        query(statement).execute(&pool).await.unwrap();
        pool.close().await;
    }

    /// Leave the store in WAL mode with a `-wal` file and no `-shm` file: the
    /// identity still loads from the main file, but the order's key cannot be
    /// read without creating the `-shm` file, so the store is unreadable.
    async fn wal_without_shm(&self) {
        // Closing the last connection removes the `-wal` and `-shm` files.
        self.sql("PRAGMA journal_mode = WAL").await;
        let dir = self.sandbox.mcli_dir();
        fs::write(dir.join("mcli.db-wal"), b"").unwrap();
        assert!(!dir.join("mcli.db-shm").exists(), "no -shm file");
    }

    fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files: Vec<_> = fs::read_dir(self.sandbox.mcli_dir())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = fs::read(&path).unwrap_or_default();
                (path, bytes)
            })
            .collect();
        files.sort();
        files
    }

    fn node(&self) -> PublicKey {
        self.fake.pubkey()
    }
}

fn document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert!(
        stdout.ends_with('\n') && stdout.lines().count() == 1,
        "{stdout}"
    );
    serde_json::from_str(&stdout).unwrap()
}

fn conforms(name: &str, outcome: &Value) {
    let path = format!("{}/docs/schemas/{name}-v1.json", env!("CARGO_MANIFEST_DIR"));
    let schema: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert!(
        schema_check::conforms(&schema, &schema, outcome),
        "{outcome}"
    );
}

fn meta(publication: &str, acceptance: &str, persistence: &str) -> Value {
    json!({"publication": publication, "daemon_acceptance": acceptance,
           "persistence": persistence, "snapshot": null})
}

mod dispute {
    use super::*;

    #[tokio::test]
    async fn an_active_order_is_disputed_and_the_local_status_follows() {
        let holder = holder(Status::Active).await;

        let run = holder
            .run(&[
                "--output",
                "json",
                "--verbose",
                "dispute",
                "open",
                "--order",
                ORDER,
                "--yes",
            ])
            .await;

        assert_eq!(
            run.code(),
            Some(0),
            "{} {}",
            run.stdout_text(),
            run.stderr_text()
        );
        assert_eq!(run.stderr_text(), "verbose: dispute.open ok\n");
        let outcome = document(&run);
        conforms("dispute-open", &outcome);
        assert_eq!(outcome["command"], "dispute.open");
        assert_eq!(
            outcome["data"],
            json!({"node_pubkey": holder.node().to_hex(), "order_id": ORDER,
                   "dispute_id": dispute_id(order_id()).to_string(),
                   "trade_pubkey": holder.buyer.public_key().to_hex()})
        );
        assert_eq!(outcome["meta"], meta("published", "accepted", "applied"));
        let received = holder.fake.received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].action, Action::Dispute);
        assert_eq!(received[0].author, holder.buyer.public_key());
        assert_eq!(received[0].order_id, Some(order_id()));
        assert_eq!(received[0].trade_index, None, "no trade index is spent");
        assert_eq!(holder.status().await, "dispute");
        assert_eq!(holder.trades.lock().unwrap()[0].status, Status::Dispute);
        assert!(!run
            .stdout_text()
            .contains(&holder.buyer.secret_key().to_secret_hex()));
    }

    #[tokio::test]
    async fn the_human_prompt_previews_and_a_refusal_shows_its_reason() {
        let holder = holder(Status::Pending).await;
        let declined = holder
            .answering(&["dispute", "open", "--order", ORDER], "n\n")
            .await;
        assert_eq!(declined.code(), Some(1));
        assert!(
            declined.stdout_text().contains("Open Dispute Preview"),
            "{}",
            declined.stdout_text()
        );
        assert!(holder.fake.received().is_empty(), "declined, nothing sent");

        let json = holder
            .run(&[
                "--output", "json", "dispute", "open", "--order", ORDER, "--yes",
            ])
            .await;
        let human = holder
            .answering(&["dispute", "open", "--order", ORDER], "y\n")
            .await;

        assert_eq!(json.code(), Some(1));
        let outcome = document(&json);
        conforms("dispute-open", &outcome);
        assert_eq!(outcome["error"]["code"], "node_refused");
        assert_eq!(outcome["error"]["reason"], "not_allowed_by_status");
        assert_eq!(
            outcome["meta"],
            meta("published", "rejected", "not_attempted")
        );
        assert_eq!(human.code(), Some(1));
        assert!(
            human
                .stderr_text()
                .contains("(reason: not_allowed_by_status)"),
            "{}",
            human.stderr_text()
        );
        assert_eq!(holder.status().await, "active", "a refusal changes nothing");
    }

    /// The node answers a second dispute with silence: uncertain, never
    /// retried, nothing changed locally.
    #[tokio::test]
    async fn a_second_dispute_is_uncertain_and_never_retried() {
        let holder = holder(Status::Active).await;
        let first = holder
            .run(&[
                "--output", "json", "dispute", "open", "--order", ORDER, "--yes",
            ])
            .await;
        assert_eq!(first.code(), Some(0));
        let before = holder.snapshot();

        let second = holder
            .run(&[
                "--output", "json", "dispute", "open", "--order", ORDER, "--yes",
            ])
            .await;

        assert_eq!(second.code(), Some(1));
        let outcome = document(&second);
        conforms("dispute-open", &outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "reply_timeout");
        assert_eq!(
            outcome["meta"],
            meta("published", "unknown", "not_attempted")
        );
        assert_eq!(holder.fake.received().len(), 2, "one request per run");
        assert_eq!(holder.snapshot(), before);
    }

    /// The node opened the dispute, so a local status that cannot follow is a
    /// warning on an `ok` outcome, with exit 0 and no retry (FR-004).
    #[tokio::test]
    async fn a_failed_status_update_still_reports_the_dispute_with_a_warning() {
        let in_json = holder(Status::Active).await;
        let in_human = holder(Status::Active).await;
        in_json.sql(FREEZE).await;
        in_human.sql(FREEZE).await;

        let json = in_json
            .run(&[
                "--output", "json", "dispute", "open", "--order", ORDER, "--yes",
            ])
            .await;
        let human = in_human
            .run(&["dispute", "open", "--order", ORDER, "--yes"])
            .await;

        assert_eq!(json.code(), Some(0), "{}", json.stderr_text());
        let outcome = document(&json);
        conforms("dispute-open", &outcome);
        assert_eq!(outcome["status"], "ok");
        assert_eq!(
            outcome["data"]["dispute_id"],
            dispute_id(order_id()).to_string()
        );
        assert_eq!(outcome["warnings"].as_array().map(Vec::len), Some(1));
        assert_eq!(outcome["warnings"][0]["code"], "local_status_stale");
        assert_eq!(outcome["meta"], meta("published", "accepted", "failed"));
        assert_eq!(human.code(), Some(0), "{}", human.stderr_text());
        assert!(
            human
                .stderr_text()
                .contains("warning: the node accepted the request but the local order status"),
            "{}",
            human.stderr_text()
        );
        for holder in [&in_json, &in_human] {
            assert_eq!(holder.status().await, "active", "the update failed");
            assert_eq!(holder.fake.received().len(), 1, "never retried");
        }
    }

    #[tokio::test]
    async fn nothing_is_sent_without_yes_in_json_or_for_an_unknown_order() {
        let holder = holder(Status::Active).await;
        let before = holder.snapshot();
        let other = "00000000-0000-0000-0000-000000000002";
        for (args, code) in [
            (
                &["--output", "json", "dispute", "open", "--order", ORDER][..],
                "invalid_arguments",
            ),
            (
                &[
                    "--output", "json", "dispute", "open", "--order", other, "--yes",
                ],
                "order_unknown",
            ),
        ] {
            let run = holder.run(args).await;
            let outcome = document(&run);
            conforms("dispute-open", &outcome);
            assert_eq!(outcome["error"]["code"], code, "{args:?}");
            assert_eq!(
                outcome["meta"],
                meta("not_attempted", "unknown", "not_attempted")
            );
        }
        assert!(holder.fake.received().is_empty());
        assert_eq!(holder.snapshot(), before);
    }

    #[tokio::test]
    async fn the_legacy_command_is_gone_with_no_alias() {
        let holder = holder(Status::Active).await;
        for args in [
            &["dispute", "--order-id", ORDER][..],
            &["dispute", "--orderid", ORDER],
            &["dispute", "-o", ORDER],
            &["dispute", "open", "--order-id", ORDER],
        ] {
            let run = holder.run(args).await;
            assert_eq!(run.code(), Some(2), "{args:?}");
        }
        assert!(holder.fake.received().is_empty());
    }
}

/// A refusal is the node's answer about this order only when it names it: a
/// `cant-do` for another order, or for none, is not a usable reply, so the
/// request may still have been acted on.
#[tokio::test]
async fn a_refusal_that_names_another_order_or_none_is_uncertain() {
    for (label, named) in [
        ("another order", Some(Uuid::from_u128(99))),
        ("no order", None),
    ] {
        let holder = holder_with(Status::Active, move |_| {
            Arc::new(move |request| {
                vec![Outgoing::to_requester(Message::cant_do(
                    named,
                    request.request_id,
                    Some(Payload::CantDo(Some(CantDoReason::NotAllowedByStatus))),
                ))]
            })
        })
        .await;
        for (args, schema) in [
            (
                &[
                    "--output", "json", "dispute", "open", "--order", ORDER, "--yes",
                ][..],
                "dispute-open",
            ),
            (
                &[
                    "--output", "json", "order", "rate", "--order", ORDER, "--rating", "5", "--yes",
                ],
                "order-rate",
            ),
        ] {
            let run = holder.run(args).await;
            let outcome = document(&run);
            conforms(schema, &outcome);
            assert_eq!(outcome["status"], "uncertain", "{label}: {args:?}");
            assert_eq!(outcome["error"]["code"], "reply_malformed", "{label}");
            assert_eq!(
                outcome["meta"],
                meta("published", "unknown", "not_attempted")
            );
        }
        assert_eq!(
            holder.status().await,
            "active",
            "{label}: nothing changed locally"
        );
    }
}

/// An acceptance, like a refusal, counts only when it names the order
/// (FR-003): a `dispute-initiated-by-you` or a `rate-received` for another
/// order, or for none, is not a usable reply, so the node may have acted and
/// nothing changes locally.
#[tokio::test]
async fn an_acceptance_that_names_another_order_or_none_is_uncertain() {
    for (label, named) in [
        ("another order", Some(Uuid::from_u128(99))),
        ("no order", None),
    ] {
        let holder = holder_with(Status::Active, move |_| {
            Arc::new(move |request| {
                let (action, payload) = match request.action {
                    Action::Dispute => (
                        Action::DisputeInitiatedByYou,
                        Payload::Dispute(Uuid::from_u128(0xd16), None),
                    ),
                    _ => (Action::RateReceived, Payload::RatingUser(5)),
                };
                vec![Outgoing::to_requester(Message::new_order(
                    named,
                    request.request_id,
                    None,
                    action,
                    Some(payload),
                ))]
            })
        })
        .await;
        for (args, schema) in [
            (
                &[
                    "--output", "json", "dispute", "open", "--order", ORDER, "--yes",
                ][..],
                "dispute-open",
            ),
            (
                &[
                    "--output", "json", "order", "rate", "--order", ORDER, "--rating", "5", "--yes",
                ],
                "order-rate",
            ),
        ] {
            let run = holder.run(args).await;
            let outcome = document(&run);
            conforms(schema, &outcome);
            assert_eq!(outcome["status"], "uncertain", "{label}: {args:?}");
            assert_eq!(outcome["error"]["code"], "reply_malformed", "{label}");
            assert_eq!(
                outcome["meta"],
                meta("published", "unknown", "not_attempted")
            );
        }
        assert_eq!(holder.fake.received().len(), 2, "{label}: one request each");
        assert_eq!(
            holder.status().await,
            "active",
            "{label}: nothing changed locally"
        );
    }
}

mod rate {
    use super::*;

    #[tokio::test]
    async fn a_successful_trade_is_rated_and_nothing_is_written_locally() {
        let holder = holder(Status::Success).await;
        let before = holder.snapshot();

        let run = holder
            .run(&[
                "--output",
                "json",
                "--verbose",
                "order",
                "rate",
                "--order",
                ORDER,
                "--rating",
                "5",
                "--yes",
            ])
            .await;

        assert_eq!(
            run.code(),
            Some(0),
            "{} {}",
            run.stdout_text(),
            run.stderr_text()
        );
        assert_eq!(run.stderr_text(), "verbose: order.rate ok\n");
        let outcome = document(&run);
        conforms("order-rate", &outcome);
        assert_eq!(
            outcome["data"],
            json!({"node_pubkey": holder.node().to_hex(), "order_id": ORDER, "rating": 5,
                   "trade_pubkey": holder.buyer.public_key().to_hex()})
        );
        assert_eq!(
            outcome["meta"],
            meta("published", "accepted", "not_attempted")
        );
        let received = holder.fake.received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].action, Action::RateUser);
        assert_eq!(received[0].rating, Some(5));
        assert_eq!(received[0].author, holder.buyer.public_key());
        assert_eq!(holder.snapshot(), before, "a rating writes nothing");
    }

    #[tokio::test]
    async fn the_human_prompt_and_a_refusal_for_a_trade_not_finished() {
        let holder = holder(Status::Active).await;
        let human = holder
            .answering(&["order", "rate", "--order", ORDER, "--rating", "4"], "y\n")
            .await;
        assert_eq!(human.code(), Some(1));
        assert!(
            human.stdout_text().contains("Rate Counterparty Preview"),
            "{}",
            human.stdout_text()
        );
        assert!(
            human
                .stderr_text()
                .contains("(reason: invalid_order_status)"),
            "{}",
            human.stderr_text()
        );
        let json = holder
            .run(&[
                "--output", "json", "order", "rate", "--order", ORDER, "--rating", "4", "--yes",
            ])
            .await;
        let outcome = document(&json);
        conforms("order-rate", &outcome);
        assert_eq!(outcome["error"]["reason"], "invalid_order_status");
    }

    #[tokio::test]
    async fn a_second_rating_is_uncertain() {
        let holder = holder(Status::Success).await;
        let args = [
            "--output", "json", "order", "rate", "--order", ORDER, "--rating", "3", "--yes",
        ];
        assert_eq!(holder.run(&args).await.code(), Some(0));

        let second = holder.run(&args).await;

        let outcome = document(&second);
        conforms("order-rate", &outcome);
        assert_eq!(outcome["status"], "uncertain");
        assert_eq!(outcome["error"]["code"], "reply_timeout");
    }

    #[tokio::test]
    async fn a_rating_outside_one_to_five_is_an_argument_error_and_nothing_is_sent() {
        let holder = holder(Status::Success).await;
        let before = holder.snapshot();
        for rating in ["0", "6", "255", "five"] {
            let json = holder
                .run(&[
                    "--output", "json", "order", "rate", "--order", ORDER, "--rating", rating,
                    "--yes",
                ])
                .await;
            let human = holder
                .run(&[
                    "order", "rate", "--order", ORDER, "--rating", rating, "--yes",
                ])
                .await;
            assert_eq!(json.code(), Some(2), "{rating}");
            let outcome = document(&json);
            assert_eq!(outcome["command"], "order.rate", "{rating}");
            assert_eq!(outcome["error"]["code"], "invalid_arguments", "{rating}");
            assert_eq!(human.code(), Some(2), "{rating}");
        }
        let missing = holder
            .run(&[
                "--output", "json", "order", "rate", "--order", ORDER, "--yes",
            ])
            .await;
        assert_eq!(missing.code(), Some(2));
        assert!(holder.fake.received().is_empty());
        assert_eq!(holder.snapshot(), before);
    }

    /// A store that cannot be read is a broken store, not an unknown order:
    /// the preview read gives the code of the send path, with or without
    /// `--yes`. `order rate` needs no writable store, so this read is its
    /// first look at the store.
    #[tokio::test]
    async fn an_unreadable_store_is_store_unwritable_with_or_without_yes() {
        let holder = holder(Status::Success).await;
        holder.wal_without_shm().await;
        let before = holder.snapshot();
        let rate = ["order", "rate", "--order", ORDER, "--rating", "5"];

        let human = holder.run(&rate).await;
        let human_yes = holder.run(&[&rate[..], &["--yes"]].concat()).await;
        let json = holder
            .run(&[&["--output", "json"][..], &rate, &["--yes"]].concat())
            .await;

        let line = "error: The local store cannot be written, so nothing was sent.\n";
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout.is_empty(), "no preview");
        assert_eq!(human.stderr_text(), line);
        assert_eq!(human_yes.code(), Some(1));
        assert_eq!(human_yes.stderr_text(), line, "the same line with --yes");
        assert_eq!(json.code(), Some(1));
        let outcome = document(&json);
        conforms("order-rate", &outcome);
        assert_eq!(outcome["error"]["code"], "store_unwritable");
        assert_eq!(
            outcome["meta"],
            meta("not_attempted", "unknown", "not_attempted")
        );
        assert!(holder.fake.received().is_empty(), "nothing is published");
        assert_eq!(holder.snapshot(), before, "no file created or changed");
    }

    #[tokio::test]
    async fn the_legacy_command_is_gone_with_no_alias() {
        let holder = holder(Status::Success).await;
        for args in [
            &["rate", "--order-id", ORDER, "--rating", "5"][..],
            &["rate", "--orderid", ORDER, "--rating", "5"],
            &["rate", "-o", ORDER, "-r", "5"],
            &["order", "rate", "-o", ORDER, "-r", "5"],
        ] {
            let run = holder.run(args).await;
            assert_eq!(run.code(), Some(2), "{args:?}");
        }
        assert!(holder.fake.received().is_empty());
    }
}
