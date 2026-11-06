//! Tests for the global `--reply-timeout SECONDS` (5 to 300, default 15): the
//! deadline of every command that waits for the node's reply. Public reads keep
//! their own limits.
//!
//! The scripted node either never answers (every command then stops at the
//! given deadline) or answers late (a longer deadline takes the answer, the
//! default does not).

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::join_all;
use mostro_core::order::{Kind as OrderKind, Status};
use nostr_sdk::prelude::{Keys, LocalRelay};
use serde_json::Value;
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;

use fake_mostro::{
    info_event, payout_invoice, seed_identity, silent, take_daemon, Book, FakeMostro, Listed, Owed,
    Respond,
};
use harness::{run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG};
use mostro_client::store::{Store, Trade};

const ORDER: &str = "00000000-0000-0000-0000-000000000001";

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

/// A sandbox with an identity and order 1 held under its own trade key.
async fn holder() -> Sandbox {
    let sandbox = Sandbox::new().unwrap();
    seed_identity(&sandbox.mcli_dir()).await;
    let trade = Trade {
        index: 1,
        keys: Keys::generate(),
    };
    let order = Owed::waiting(Uuid::from_u128(1), trade.keys.public_key()).small(Status::Active);
    Store::in_dir(&sandbox.mcli_dir())
        .save_order(&order, &trade, 1)
        .await
        .unwrap();
    sandbox
}

/// Run `args` against `node` and time it.
async fn timed(sandbox: &Sandbox, node: &Node, args: &[String]) -> (Captured, Duration) {
    let mut command = sandbox.command(args);
    command
        .env("MOSTRO_PUBKEY", node.fake.pubkey().to_hex())
        .env("RELAYS", &node.relay.url);
    let start = Instant::now();
    let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
    assert!(run.reaped && run.status.is_some(), "{args:?}");
    (run, start.elapsed())
}

fn document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert!(stdout.lines().count() == 1, "{stdout}");
    serde_json::from_str(&stdout).unwrap()
}

fn words(text: &str) -> Vec<String> {
    text.split(' ').map(str::to_owned).collect()
}

/// Every command that waits for the node stops at the given deadline (5 s)
/// instead of the default 15 s, with the documented reply-timeout outcome.
#[tokio::test]
async fn every_command_that_waits_for_the_node_stops_at_the_given_deadline() {
    let node = node(silent()).await;
    let bond = payout_invoice(5_000);
    let commands = [
        "order new --side sell --currency USD --fiat-amount 10 --payment-method bank --yes"
            .to_string(),
        format!("order take-sell --order {ORDER} --yes"),
        format!("order take-buy --order {ORDER} --yes"),
        format!("order add-invoice --order {ORDER} --invoice lnbcrt1late --yes"),
        format!("bond add-invoice --order {ORDER} --invoice {bond} --yes"),
        format!("order fiat-sent --order {ORDER} --yes"),
        format!("order release --order {ORDER} --yes"),
        format!("order cancel --order {ORDER} --yes"),
        format!("order info --order {ORDER}"),
        format!("dispute open --order {ORDER} --yes"),
        format!("order rate --order {ORDER} --rating 5 --yes"),
    ];
    let mut sandboxes = Vec::new();
    for _ in &commands {
        sandboxes.push(holder().await);
    }
    let runs = join_all(commands.iter().zip(&sandboxes).map(|(command, sandbox)| {
        // The option is accepted before, between and after the command words.
        let mut args = words("--output json");
        args.extend(words(command));
        args.extend(words("--reply-timeout 5"));
        let node = &node;
        async move { (command.clone(), timed(sandbox, node, &args).await) }
    }))
    .await;

    for (command, (run, elapsed)) in runs {
        assert_eq!(run.code(), Some(1), "{command}: {}", run.stdout_text());
        let outcome = document(&run);
        assert_eq!(outcome["error"]["code"], "reply_timeout", "{command}");
        let expected = if command.starts_with("order info") {
            "error"
        } else {
            "uncertain"
        };
        assert_eq!(outcome["status"], expected, "{command}");
        assert!(
            elapsed >= Duration::from_secs(5) && elapsed < Duration::from_secs(12),
            "{command}: took {elapsed:?}"
        );
    }
}

/// A node that answers after 16 s: `--reply-timeout 20` takes the answer; the
/// default 15 s deadline does not, as before.
#[tokio::test]
async fn a_late_answer_is_taken_with_a_longer_deadline_and_missed_by_the_default() {
    let late = |respond: Respond| -> Respond {
        Arc::new(move |request| {
            respond(request)
                .into_iter()
                .map(|outgoing| outgoing.after(Duration::from_secs(16)))
                .collect()
        })
    };
    let book = || Book {
        orders: vec![Listed::fixed(
            Uuid::from_u128(1),
            OrderKind::Sell,
            Keys::generate().public_key(),
        )],
        last_trade_index: 0,
        ..Book::default()
    };
    let (longer, default) = (
        node(late(take_daemon(book(), false).0)).await,
        node(late(take_daemon(book(), false).0)).await,
    );
    let (one, two) = (holder().await, holder().await);
    let take = words(&format!(
        "--output json order take-sell --order {ORDER} --yes"
    ));
    let mut with_flag = words("--reply-timeout 20");
    with_flag.extend(take.clone());

    let ((patient, _), (hasty, _)) = tokio::join!(
        timed(&one, &longer, &with_flag),
        timed(&two, &default, &take)
    );

    assert_eq!(patient.code(), Some(0), "{}", patient.stdout_text());
    assert_eq!(document(&patient)["data"]["next_step"], "add_invoice");
    assert_eq!(hasty.code(), Some(1));
    let outcome = document(&hasty);
    assert_eq!(outcome["status"], "uncertain");
    assert_eq!(outcome["error"]["code"], "reply_timeout");
}

/// Outside 5 to 300 seconds, or not a number: `invalid_arguments` (exit 2)
/// with the command named, before anything runs.
#[tokio::test]
async fn a_deadline_out_of_range_is_an_argument_error_before_any_effect() {
    let node = node(silent()).await;
    let sandbox = holder().await;
    for value in ["4", "301", "0", "soon", ""] {
        for args in [
            vec![
                "--output",
                "json",
                "--reply-timeout",
                value,
                "order",
                "take-sell",
                "--order",
                ORDER,
                "--yes",
            ],
            vec![
                "--output",
                "json",
                "order",
                "take-sell",
                "--order",
                ORDER,
                "--yes",
                "--reply-timeout",
                value,
            ],
        ] {
            let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
            let (run, _) = timed(&sandbox, &node, &args).await;
            assert_eq!(run.code(), Some(2), "{args:?}");
            let outcome = document(&run);
            assert_eq!(outcome["error"]["code"], "invalid_arguments", "{args:?}");
            assert_eq!(outcome["command"], "order.take-sell", "{args:?}");
        }
    }
    assert!(node.fake.received().is_empty(), "nothing was sent");
}

/// A public read keeps its own limits; the option is accepted and has no
/// effect there.
#[tokio::test]
async fn a_public_read_accepts_the_option_and_keeps_its_own_limits() {
    let node = node(silent()).await;
    let sandbox = Sandbox::new().unwrap();
    let args = words("--output json order list --reply-timeout 5");
    let (run, _) = timed(&sandbox, &node, &args).await;
    assert_eq!(run.code(), Some(0), "{}", run.stdout_text());
    assert_eq!(document(&run)["data"]["orders"], Value::Array(Vec::new()));
}
