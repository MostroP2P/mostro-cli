//! Process tests for `admin order-info` (spec 023): the real binary against
//! the scripted node on a loopback relay.
//!
//! Every run is checked for the admin key: neither its `nsec` nor its hex may
//! appear on stdout or stderr (FR-011).

use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mostro_core::prelude::{
    Action, BondInfo, BondKind, BondRole, BondState, CantDoReason, HoldInvoiceInfo,
    HoldInvoiceState, Message, OrderInfo, Payload,
};
use nostr_sdk::prelude::{Keys, LocalRelay, ToBech32};
use serde_json::{json, Value};
use uuid::Uuid;

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{
    info_event, order_info_reply, refusal, silent, FakeMostro, Outgoing, Received, Respond,
};
use harness::{
    is_empty_dir, run_with_watchdog, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG,
};

const SCHEMA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schemas/admin-order-info-v1.json"
);
const ORDER: Uuid = Uuid::from_u128(1);

/// An order whose payout failed: the hold invoice is settled, the payout
/// marker is gone and the database flags tell what happened (Story 1).
fn stuck_order() -> OrderInfo {
    OrderInfo {
        id: ORDER,
        kind: "sell".to_string(),
        status: "settled-hold-invoice".to_string(),
        amount: 100_000,
        fee: 600,
        fiat_code: "EUR".to_string(),
        fiat_amount: 50,
        payment_method: "SEPA".to_string(),
        premium: 1,
        created_at: 1_792_108_800,
        taken_at: Some(1_792_108_900),
        expires_at: 1_792_195_200,
        buyer_trade_pubkey: Some(Keys::generate().public_key().to_hex()),
        seller_trade_pubkey: Some(Keys::generate().public_key().to_hex()),
        failed_payment: true,
        payment_attempts: 2,
        hold_settled_at: Some(1_792_109_050),
        hold_invoice: Some(HoldInvoiceInfo {
            state: HoldInvoiceState::Settled,
            held_at: Some(1_792_109_000),
        }),
        bonds: vec![BondInfo {
            role: BondRole::Taker,
            amount: 1_000,
            state: BondState::Locked,
            slash_reason: None,
            kind: BondKind::Bond,
            root_order_id: None,
        }],
        ..Default::default()
    }
}

/// Is `request` what the node answers: `admin-order-info` with no payload?
/// The node also sees its own replies when the admin is the node (Story 5).
fn is_query(request: &Received) -> bool {
    request.action == Action::AdminOrderInfo && !request.has_payload
}

/// A node that answers every query with `info`.
fn answers(info: OrderInfo) -> Respond {
    Arc::new(move |request: &Received| {
        if !is_query(request) {
            return Vec::new();
        }
        vec![Outgoing::to_requester(order_info_reply(
            request,
            info.clone(),
        ))]
    })
}

/// A node that refuses every query with `reason`.
fn refuses(reason: CantDoReason) -> Respond {
    Arc::new(move |request: &Received| {
        if !is_query(request) {
            return Vec::new();
        }
        vec![Outgoing::to_requester(refusal(request, reason.clone()))]
    })
}

/// The scripted node on a loopback relay, and the admin who asks it.
struct World {
    relay: LoopbackRelay,
    node: FakeMostro,
    admin: Keys,
    sandbox: Sandbox,
}

async fn world(respond: Respond) -> World {
    world_as(respond, |_| Keys::generate()).await
}

/// `admin` picks the asker's key from the node's.
async fn world_as(respond: Respond, admin: impl FnOnce(&Keys) -> Keys) -> World {
    let node = Keys::generate();
    let relay = LocalRelay::new();
    relay
        .add_event(info_event(&node, Some(2), Some(0), None, Some(10)))
        .await
        .unwrap();
    let relay = LoopbackRelay::start(Backend::Relay(Box::new(relay)))
        .await
        .unwrap();
    let admin = admin(&node);
    let node = FakeMostro::start(&relay.url, node, respond).await;
    World {
        relay,
        node,
        admin,
        sandbox: Sandbox::new().unwrap(),
    }
}

impl World {
    fn nsec(&self) -> String {
        self.admin.secret_key().to_bech32().unwrap()
    }

    /// Run with the node configured and the admin key in `ADMIN_NSEC`.
    async fn run(&self, args: &[&str]) -> Captured {
        self.run_env(args, &[("ADMIN_NSEC", &self.nsec())]).await
    }

    /// Run with the node configured and only the variables of `set` added.
    async fn run_env(&self, args: &[&str], set: &[(&str, &str)]) -> Captured {
        let mut command = self.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", self.node.pubkey().to_hex())
            .env("RELAYS", &self.relay.url);
        for (name, value) in set {
            command.env(name, value);
        }
        let run = run_with_watchdog(command, WATCHDOG).await.unwrap();
        assert!(run.reaped && run.status.is_some(), "{args:?}");
        // FR-011: the admin key appears in no output.
        for secret in [self.nsec(), self.admin.secret_key().to_secret_hex()] {
            assert!(!run.stdout_text().contains(&secret), "{args:?}");
            assert!(!run.stderr_text().contains(&secret), "{args:?}");
        }
        run
    }

    /// The queries the node received, without its own replies.
    fn queries(&self) -> Vec<Received> {
        self.node.received().into_iter().filter(is_query).collect()
    }

    /// FR-003: no store was opened and nothing was created.
    fn assert_nothing_created(&self) {
        for dir in [
            self.sandbox.mcli_dir(),
            self.sandbox.home(),
            self.sandbox.cwd(),
        ] {
            assert!(is_empty_dir(&dir), "{dir:?}");
        }
    }
}

fn order_arg() -> String {
    ORDER.to_string()
}

fn json_args(order: &str) -> Vec<&str> {
    vec!["--output", "json", "admin", "order-info", "--order", order]
}

fn human_args(order: &str) -> Vec<&str> {
    vec!["admin", "order-info", "--order", order]
}

/// The one JSON document of a run, checked against the command's schema
/// (SC-002).
fn document(run: &Captured) -> Value {
    let stdout = run.stdout_text();
    assert!(
        stdout.ends_with('\n') && stdout.lines().count() == 1,
        "{stdout}"
    );
    let outcome: Value = serde_json::from_str(&stdout).unwrap();
    let schema: Value = serde_json::from_str(&fs::read_to_string(SCHEMA).unwrap()).unwrap();
    assert!(
        schema_check::conforms(&schema, &schema, &outcome),
        "not in the schema: {outcome}"
    );
    outcome
}

const PUBLISHED_ACCEPTED: &str = r#"{"publication":"published","daemon_acceptance":"accepted","persistence":"not_applicable","snapshot":null}"#;

// --- Story 1: a solver looks at a stuck order ------------------------------

#[tokio::test]
async fn a_solver_gets_the_snapshot_of_a_stuck_order() {
    let info = stuck_order();
    let world = world(answers(info.clone())).await;
    let order = order_arg();

    let run = world.run(&json_args(&order)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(run.stderr.is_empty(), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["command"], "admin.order-info");
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["error"], Value::Null);
    assert_eq!(
        outcome["meta"],
        serde_json::from_str::<Value>(PUBLISHED_ACCEPTED).unwrap()
    );
    let data = &outcome["data"];
    assert_eq!(data["node_pubkey"], world.node.pubkey().to_hex());
    assert_eq!(data["order_id"], order);
    assert_eq!(data["side"], "sell");
    assert_eq!(data["status"], "settled-hold-invoice");
    assert_eq!(data["sats_amount"], "100000");
    assert_eq!(data["fee_sats"], "600");
    assert_eq!(data["buyer_trade_pubkey"], json!(info.buyer_trade_pubkey));
    assert_eq!(data["seller_trade_pubkey"], json!(info.seller_trade_pubkey));
    assert_eq!(data["taken_at"], "2026-10-16T00:01:40Z");
    // The payout marker is gone after a failure: the flags tell the story.
    assert_eq!(data["payout"], Value::Null);
    assert_eq!(data["failed_payment"], true);
    assert_eq!(data["payment_attempts"], "2");
    assert_eq!(data["hold_settled_at_unix"], "1792109050");
    assert_eq!(
        data["hold_invoice"],
        json!({"state": "settled", "held_at": "2026-10-16T00:03:20Z",
               "held_at_unix": "1792109000"})
    );
    assert_eq!(data["cashu_escrow"], Value::Null);
    assert_eq!(
        data["bonds"],
        json!([{"role": "taker", "amount_sats": "1000", "state": "locked",
                "slash_reason": null, "kind": "bond", "root_order_id": null}])
    );
    world.assert_nothing_created();
}

/// FR-001: one request, in the `order` wrapper's shape, signed by the admin
/// key as identity and author.
#[tokio::test]
async fn the_request_names_the_order_carries_no_payload_and_is_signed_by_the_admin() {
    let world = world(answers(stuck_order())).await;

    let run = world.run(&json_args(&order_arg())).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    let queries = world.queries();
    assert_eq!(queries.len(), 1, "one request, never repeated");
    let query = &queries[0];
    assert_eq!(query.action, Action::AdminOrderInfo);
    assert_eq!(query.order_id, Some(ORDER));
    assert!(!query.has_payload);
    assert!(query.request_id.is_some(), "the reply is matched on it");
    assert_eq!(query.author, world.admin.public_key());
    assert_eq!(query.identity, world.admin.public_key());
    assert!(query.signature_valid);
    assert_eq!(query.trade_index, None);
}

#[tokio::test]
async fn human_mode_prints_the_same_snapshot() {
    let mut info = stuck_order();
    info.payment_method = "SEPA\u{1b}[31m instant".to_string();
    let world = world(answers(info)).await;
    let order = order_arg();

    let human = world.run(&human_args(&order)).await;
    let explicit = world
        .run(&[
            "--output",
            "human",
            "admin",
            "order-info",
            "--order",
            &order,
        ])
        .await;

    assert_eq!((human.code(), explicit.code()), (Some(0), Some(0)));
    assert!(human.stderr.is_empty(), "{}", human.stderr_text());
    let text = human.stdout_text();
    // The buyer and seller keys differ per run; the rest is the same text.
    assert_eq!(text.lines().count(), explicit.stdout_text().lines().count());
    for needle in [
        format!("🔎 Order {order}").as_str(),
        "Status: settled-hold-invoice",
        "Amount: 100000 sats",
        "Payment Method: SEPA\\u{1b}[31m instant",
        "Taken: 2026-10-16 00:01:40 UTC",
        "Failed payment: yes (attempts: 2)",
        "Escrow: hold invoice",
        "State: settled",
        "Bonds",
        "1. taker  1000 sats  locked  (bond)",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
    assert!(!text.contains("Payout"), "no payout is in flight:\n{text}");
    assert!(
        !text.contains('\u{1b}'),
        "a raw control character reached the terminal"
    );
    world.assert_nothing_created();
}

// --- Story 2: the Lightning node is down -----------------------------------

#[tokio::test]
async fn a_node_whose_lightning_node_is_down_still_answers_for_the_order() {
    let world = world(answers(OrderInfo {
        status: "active".to_string(),
        failed_payment: false,
        payment_attempts: 0,
        hold_settled_at: None,
        hold_invoice: Some(HoldInvoiceInfo {
            state: HoldInvoiceState::Unavailable,
            held_at: Some(1_792_109_000),
        }),
        ..stuck_order()
    }))
    .await;
    let order = order_arg();

    let run = world.run(&json_args(&order)).await;
    let human = world.run(&human_args(&order)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["data"]["status"], "active");
    assert_eq!(outcome["data"]["hold_invoice"]["state"], "unavailable");
    assert_eq!(
        outcome["data"]["hold_invoice"]["held_at_unix"],
        "1792109000"
    );
    assert_eq!(outcome["data"]["sats_amount"], "100000");
    assert!(
        human
            .stdout_text()
            .contains("State: unavailable (the node could not ask its Lightning node)"),
        "{}",
        human.stdout_text()
    );
}

/// SC-004: a node newer than this program.
#[tokio::test]
async fn a_status_this_program_does_not_know_is_shown_as_the_node_sent_it() {
    let world = world(answers(OrderInfo {
        status: "a-status-added-later".to_string(),
        ..stuck_order()
    }))
    .await;
    let order = order_arg();

    let run = world.run(&json_args(&order)).await;
    let human = world.run(&human_args(&order)).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert_eq!(document(&run)["data"]["status"], "a-status-added-later");
    assert!(human.stdout_text().contains("Status: a-status-added-later"));
}

// --- Story 3: a key that may not ask, and an order that does not exist -----

#[tokio::test]
async fn a_refusal_is_node_refused_with_the_reason_in_both_modes() {
    for (reason, name) in [
        (CantDoReason::InvalidPubkey, "invalid_pubkey"),
        (CantDoReason::NotFound, "not_found"),
    ] {
        let world = world(refuses(reason)).await;
        let order = order_arg();

        let run = world.run(&json_args(&order)).await;
        assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
        let outcome = document(&run);
        assert_eq!(outcome["status"], "error", "{name}");
        assert_eq!(outcome["data"], Value::Null);
        assert_eq!(outcome["error"]["code"], "node_refused");
        assert_eq!(outcome["error"]["reason"], name);
        assert_eq!(
            outcome["meta"],
            json!({"publication": "published", "daemon_acceptance": "rejected",
                   "persistence": "not_applicable", "snapshot": null})
        );

        let human = world.run(&human_args(&order)).await;
        assert_eq!(human.code(), Some(1));
        assert!(human.stdout.is_empty());
        assert_eq!(
            human.stderr_text(),
            format!("error: The node refused the request. (reason: {name})\n")
        );
        world.assert_nothing_created();
    }
}

// --- Story 4: no admin key -------------------------------------------------

/// SC-003: fails before any relay is contacted, and creates nothing.
#[tokio::test]
async fn without_the_admin_key_nothing_is_sent() {
    let world = world(answers(stuck_order())).await;
    let order = order_arg();

    for set in [&[][..], &[("ADMIN_NSEC", "")][..], &[("ADMIN_NSEC", "  ")]] {
        let run = world.run_env(&json_args(&order), set).await;
        assert_eq!(run.code(), Some(1), "{set:?}");
        let outcome = document(&run);
        assert_eq!(outcome["error"]["code"], "admin_key_missing", "{set:?}");
        assert_eq!(outcome["error"]["message"], "ADMIN_NSEC is not set.");
        assert_eq!(outcome["meta"]["publication"], "not_attempted");
        assert_eq!(outcome["meta"]["daemon_acceptance"], "unknown");
    }
    let human = world.run_env(&human_args(&order), &[]).await;
    assert_eq!(human.code(), Some(1));
    assert!(human.stdout.is_empty());
    assert_eq!(human.stderr_text(), "error: ADMIN_NSEC is not set.\n");

    assert!(world.queries().is_empty());
    assert_eq!(world.relay.connections(), 1, "only the scripted node's own");
    world.assert_nothing_created();
}

#[tokio::test]
async fn a_value_that_is_not_a_private_key_is_refused_without_being_echoed() {
    const SENTINEL: &str = "secret-sentinel-7f3a";
    let world = world(answers(stuck_order())).await;
    let order = order_arg();
    let public = world.admin.public_key().to_bech32().unwrap();

    for value in [SENTINEL, public.as_str()] {
        let run = world
            .run_env(&json_args(&order), &[("ADMIN_NSEC", value)])
            .await;
        assert_eq!(run.code(), Some(1));
        assert_eq!(document(&run)["error"]["code"], "admin_key_invalid");
        let human = world
            .run_env(&human_args(&order), &[("ADMIN_NSEC", value)])
            .await;
        assert_eq!(
            human.stderr_text(),
            "error: ADMIN_NSEC is not a private key.\n"
        );
        for stream in [run.stdout_text(), run.stderr_text(), human.stdout_text()] {
            assert!(!stream.contains(value), "{stream}");
        }
    }
    assert!(world.queries().is_empty());
    assert_eq!(world.relay.connections(), 1, "only the scripted node's own");
    world.assert_nothing_created();
}

/// FR-002: the node configuration is checked before the admin key.
#[tokio::test]
async fn a_missing_node_configuration_is_reported_before_the_admin_key() {
    let sandbox = Sandbox::new().unwrap();
    let order = order_arg();

    let run = run_with_watchdog(sandbox.command(json_args(&order)), WATCHDOG)
        .await
        .unwrap();

    assert_eq!(run.code(), Some(1));
    let outcome = document(&run);
    assert_eq!(outcome["error"]["code"], "configuration_missing");
    assert_eq!(outcome["command"], "admin.order-info");
}

// --- Story 5: the admin is the node ----------------------------------------

/// The request is authored by the node's key and addressed to it, so it
/// matches the reply filter itself. The reply must be the snapshot.
#[tokio::test]
async fn when_the_admin_is_the_node_the_reply_is_not_its_own_request() {
    let world = world_as(answers(stuck_order()), Keys::clone).await;
    assert_eq!(world.admin.public_key(), world.node.pubkey());

    let run = world.run(&json_args(&order_arg())).await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["status"], "ok");
    assert_eq!(outcome["data"]["status"], "settled-hold-invoice");
    assert_eq!(world.queries().len(), 1);
}

// --- Replies that cannot be used -------------------------------------------

#[tokio::test]
async fn a_reply_that_is_not_the_snapshot_of_this_order_is_reply_malformed() {
    let other = Uuid::from_u128(2);
    let scripts: [(&str, Respond); 3] = [
        (
            "another payload",
            Arc::new(|request: &Received| {
                vec![Outgoing::to_requester(Message::new_order(
                    request.order_id,
                    request.request_id,
                    None,
                    Action::AdminOrderInfo,
                    Some(Payload::TextMessage("not a snapshot".to_string())),
                ))]
            }),
        ),
        (
            "a snapshot of another order",
            Arc::new(move |request: &Received| {
                vec![Outgoing::to_requester(Message::new_order(
                    request.order_id,
                    request.request_id,
                    None,
                    Action::AdminOrderInfo,
                    Some(Payload::OrderInfo(OrderInfo {
                        id: other,
                        ..stuck_order()
                    })),
                ))]
            }),
        ),
        (
            "a message about another order",
            Arc::new(move |request: &Received| {
                vec![Outgoing::to_requester(Message::new_order(
                    Some(other),
                    request.request_id,
                    None,
                    Action::AdminOrderInfo,
                    Some(Payload::OrderInfo(OrderInfo {
                        id: other,
                        ..stuck_order()
                    })),
                ))]
            }),
        ),
    ];
    for (label, respond) in scripts {
        let world = world(respond).await;

        let run = world.run(&json_args(&order_arg())).await;

        assert_eq!(run.code(), Some(1), "{label}");
        let outcome = document(&run);
        assert_eq!(outcome["error"]["code"], "reply_malformed", "{label}");
        assert_eq!(outcome["data"], Value::Null, "{label}");
        assert_eq!(outcome["meta"]["publication"], "published");
        assert_eq!(outcome["meta"]["daemon_acceptance"], "unknown");
    }
}

/// A node that does not know the action answers nothing.
#[tokio::test]
async fn a_silent_node_is_a_reply_timeout_after_the_reply_budget() {
    let world = world(silent()).await;
    let order = order_arg();
    let started = Instant::now();

    let run = world
        .run(&[
            "--reply-timeout",
            "5",
            "--output",
            "json",
            "admin",
            "order-info",
            "--order",
            &order,
        ])
        .await;

    assert!(
        started.elapsed() >= Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(run.code(), Some(1), "{}", run.stderr_text());
    let outcome = document(&run);
    assert_eq!(outcome["error"]["code"], "reply_timeout");
    assert_eq!(outcome["meta"]["publication"], "published");
    assert_eq!(outcome["meta"]["daemon_acceptance"], "unknown");
    assert_eq!(world.queries().len(), 1, "never retried");
    world.assert_nothing_created();
}

/// A reply from another key, or with another request id, is not the answer.
#[tokio::test]
async fn a_snapshot_from_another_key_is_ignored() {
    let impostor = Keys::generate();
    let respond: Respond = Arc::new(move |request: &Received| {
        vec![
            Outgoing::to_requester(order_info_reply(request, stuck_order()))
                .signed_by(impostor.clone()),
        ]
    });
    let world = world(respond).await;
    let order = order_arg();

    let run = world
        .run(&[
            "--reply-timeout",
            "5",
            "--output",
            "json",
            "admin",
            "order-info",
            "--order",
            &order,
        ])
        .await;

    assert_eq!(run.code(), Some(1));
    assert_eq!(document(&run)["error"]["code"], "reply_timeout");
}

// --- Arguments -------------------------------------------------------------

#[tokio::test]
async fn a_missing_repeated_or_malformed_order_is_invalid_arguments() {
    let world = world(answers(stuck_order())).await;
    let order = order_arg();
    let cases: [Vec<&str>; 4] = [
        vec!["--output", "json", "admin", "order-info"],
        vec!["--output", "json", "admin", "order-info", "--order", "nope"],
        vec![
            "--output",
            "json",
            "admin",
            "order-info",
            "--order",
            &order,
            "--order",
            &order,
        ],
        vec!["--output", "json", "admin", "order-info", "--order"],
    ];

    for args in cases {
        let run = world.run(&args).await;
        assert_eq!(run.code(), Some(2), "{args:?}");
        let outcome = document(&run);
        assert_eq!(outcome["error"]["code"], "invalid_arguments", "{args:?}");
        assert_eq!(outcome["command"], "admin.order-info", "{args:?}");
    }
    assert!(world.queries().is_empty());
    world.assert_nothing_created();
}

/// Discovery describes the command and what it needs.
#[tokio::test]
async fn schema_lists_the_command_with_its_configuration() {
    let sandbox = Sandbox::new().unwrap();

    let run = run_with_watchdog(sandbox.command(["--output", "json", "schema"]), WATCHDOG)
        .await
        .unwrap();

    assert_eq!(run.code(), Some(0));
    let outcome: Value = serde_json::from_str(&run.stdout_text()).unwrap();
    let command = outcome["data"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "admin.order-info")
        .expect("the command is listed");
    assert_eq!(command["cli_path"], json!(["admin", "order-info"]));
    assert_eq!(command["output_modes"], json!(["human", "json"]));
    assert_eq!(
        command["required_configuration"],
        json!(["MOSTRO_PUBKEY", "RELAYS", "ADMIN_NSEC"])
    );
    assert_eq!(command["input_schema"]["required"], json!(["order"]));
    let errors = command["output_schema"].to_string();
    for code in ["admin_key_missing", "admin_key_invalid", "node_refused"] {
        assert!(errors.contains(code), "{code}");
    }
    assert!(!errors.contains("identity_missing"));
}
