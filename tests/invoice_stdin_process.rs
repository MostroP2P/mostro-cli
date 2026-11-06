//! Tests for `--invoice -` (spec 010): the invoice read from standard input by
//! `order add-invoice`, `bond add-invoice`, `order take-sell` and `order new`.
//!
//! Every run is the real binary against the scripted node on a loopback relay,
//! in a sandbox with a seeded identity. What the node received is the proof of
//! what was sent; the store's bytes are the proof that nothing was written.

use std::path::PathBuf;
use std::time::Duration;

use mostro_core::order::{Kind as OrderKind, Status};
use nostr_sdk::prelude::{Keys, LocalRelay};
use serde_json::{json, Value};

#[path = "common/fake_mostro.rs"]
mod fake_mostro;
#[path = "common/harness.rs"]
mod harness;
#[path = "common/schema_check.rs"]
mod schema_check;

use fake_mostro::{
    info_event, invoice_daemon, new_order_daemon, payout_invoice, seed_identity, take_daemon,
    BondOwed, Book, FakeMostro, Ledger, Listed, Owed, Received, Respond,
};
use harness::{
    run_with_input, run_with_open_stdin, Backend, Captured, LoopbackRelay, Sandbox, WATCHDOG,
};
use mostro_client::store::{Store, Trade};
use uuid::Uuid;

/// A BOLT-11-shaped invoice that must never come back on any stream.
const INVOICE: &str = "lnbcrt1stdinbuyerinvoiceSENTINEL";
const SHARE: i64 = 5_000;
const REQUEST_ID: u64 = 8150;
/// Long enough for a child that waits for input to show it; far shorter than
/// the watchdog.
const NO_WAIT: Duration = Duration::from_secs(10);

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// The four commands that take `--invoice`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    AddInvoice,
    BondInvoice,
    TakeSell,
    New,
}

impl Kind {
    const ALL: [Kind; 4] = [
        Kind::AddInvoice,
        Kind::BondInvoice,
        Kind::TakeSell,
        Kind::New,
    ];

    fn canonical(self) -> &'static str {
        match self {
            Self::AddInvoice => "order.add-invoice",
            Self::BondInvoice => "bond.add-invoice",
            Self::TakeSell => "order.take-sell",
            Self::New => "order.new",
        }
    }

    /// The invoice the scripted node accepts for this command.
    fn invoice(self) -> String {
        match self {
            Self::BondInvoice => payout_invoice(SHARE),
            _ => INVOICE.to_string(),
        }
    }

    /// The command words and options, with `--invoice <value>` and `tail`.
    fn args(self, value: &str, tail: &[&str]) -> Vec<String> {
        let words: Vec<&str> = match self {
            Self::AddInvoice => vec![
                "order",
                "add-invoice",
                "--order",
                "00000000-0000-0000-0000-000000000001",
            ],
            Self::BondInvoice => vec![
                "bond",
                "add-invoice",
                "--order",
                "00000000-0000-0000-0000-000000000009",
            ],
            Self::TakeSell => vec![
                "order",
                "take-sell",
                "--order",
                "00000000-0000-0000-0000-000000000001",
            ],
            Self::New => vec![
                "order",
                "new",
                "--side",
                "buy",
                "--currency",
                "USD",
                "--fiat-amount",
                "10",
                "--payment-method",
                "bank transfer",
            ],
        };
        words
            .into_iter()
            .chain(["--invoice", value])
            .chain(tail.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// The invoice the node received in the request, if any.
    fn received_invoice(self, received: &Received) -> Option<String> {
        match self {
            Self::New => received
                .order
                .as_ref()
                .and_then(|order| order.buyer_invoice.clone()),
            _ => received.invoice.clone(),
        }
    }
}

/// The scripted node for one command and a user who holds what it needs.
struct World {
    kind: Kind,
    relay: LoopbackRelay,
    fake: FakeMostro,
    sandbox: Sandbox,
    buyer: Keys,
}

async fn world(kind: Kind) -> World {
    let buyer = Keys::generate();
    let respond: Respond = match kind {
        Kind::AddInvoice | Kind::BondInvoice => {
            let ledger = Ledger {
                orders: vec![Owed::waiting(id(1), buyer.public_key())],
                bonds: vec![BondOwed {
                    order: id(9),
                    recipient: buyer.public_key(),
                    share: SHARE,
                    open: true,
                    invoice: None,
                }],
            };
            invoice_daemon(ledger, true).0
        }
        Kind::TakeSell => {
            let book = Book {
                orders: vec![Listed::fixed(
                    id(1),
                    OrderKind::Sell,
                    Keys::generate().public_key(),
                )],
                last_trade_index: 0,
                ..Book::default()
            };
            take_daemon(book, false).0
        }
        Kind::New => new_order_daemon(0, false).0,
    };
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
    if matches!(kind, Kind::AddInvoice | Kind::BondInvoice) {
        let store = Store::in_dir(&sandbox.mcli_dir());
        let trade = Trade {
            index: 1,
            keys: buyer.clone(),
        };
        for order in [id(1), id(9)] {
            let small = Owed::waiting(order, buyer.public_key()).small(Status::WaitingBuyerInvoice);
            store.save_order(&small, &trade, REQUEST_ID).await.unwrap();
        }
    }
    World {
        kind,
        relay,
        fake,
        sandbox,
        buyer,
    }
}

impl World {
    fn command(&self, front: &[&str], value: &str, tail: &[&str]) -> tokio::process::Command {
        let mut args: Vec<String> = front.iter().map(|word| word.to_string()).collect();
        args.extend(self.kind.args(value, tail));
        let mut command = self.sandbox.command(args);
        command
            .env("MOSTRO_PUBKEY", self.fake.pubkey().to_hex())
            .env("RELAYS", &self.relay.url);
        command
    }

    /// `--invoice -` with `input` on stdin.
    async fn piped(&self, front: &[&str], tail: &[&str], input: &[u8]) -> Captured {
        let run = run_with_input(self.command(front, "-", tail), input.to_vec(), WATCHDOG)
            .await
            .unwrap();
        assert!(
            run.reaped && run.status.is_some(),
            "{:?} {front:?}",
            self.kind
        );
        run
    }

    fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(self.sandbox.mcli_dir())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = std::fs::read(&path).unwrap_or_default();
                (path, bytes)
            })
            .collect();
        files.sort();
        files
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

fn assert_never_shown(run: &Captured, invoice: &str, label: &str) {
    for stream in [run.stdout_text(), run.stderr_text()] {
        assert!(
            !stream.contains(invoice),
            "{label}: the invoice came back: {stream}"
        );
        assert!(
            !stream.contains("SENTINEL"),
            "{label}: the invoice came back: {stream}"
        );
    }
}

/// With `--yes`, the invoice on stdin is sent exactly as a literal one would
/// be, in JSON and in human mode, and it never comes back.
#[tokio::test]
async fn the_invoice_on_stdin_is_the_one_sent_by_every_command() {
    for kind in Kind::ALL {
        let world = world(kind).await;
        let invoice = kind.invoice();
        let input = format!("{invoice}\n");

        let run = world
            .piped(&["--output", "json"], &["--yes"], input.as_bytes())
            .await;

        let label = kind.canonical();
        assert_eq!(
            run.code(),
            Some(0),
            "{label}: {} {}",
            run.stdout_text(),
            run.stderr_text()
        );
        let outcome = document(&run);
        assert_eq!(outcome["command"], label);
        assert_eq!(outcome["status"], "ok", "{label}");
        let received = world.fake.received();
        assert_eq!(received.len(), 1, "{label}");
        assert_eq!(
            kind.received_invoice(&received[0]).as_deref(),
            Some(invoice.as_str()),
            "{label}"
        );
        assert_never_shown(&run, &invoice, label);
    }
}

/// Surrounding ASCII whitespace and a CRLF are trimmed; human mode needs
/// `--yes` too and then asks nothing.
#[tokio::test]
async fn human_mode_with_yes_reads_one_trimmed_line() {
    let world = world(Kind::AddInvoice).await;

    let run = world
        .piped(
            &["--verbose"],
            &["--yes"],
            format!(" \t{INVOICE} \r\n").as_bytes(),
        )
        .await;

    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(
        run.stdout_text().contains("Invoice Accepted"),
        "{}",
        run.stdout_text()
    );
    assert_eq!(run.stderr_text(), "verbose: order.add-invoice ok\n");
    assert_eq!(world.fake.received()[0].invoice.as_deref(), Some(INVOICE));
    assert_never_shown(&run, INVOICE, "human");
    assert!(world.buyer.public_key().to_hex().len() == 64);
}

/// Empty, blank, two lines, more than 8 KiB and invalid UTF-8 are all
/// `invalid_arguments` before anything is sent or written, and nothing read
/// is echoed.
#[tokio::test]
async fn invalid_input_on_stdin_is_an_argument_error_before_any_effect() {
    let long = format!("{INVOICE}{}", "q".repeat(8192));
    let mut not_utf8 = INVOICE.as_bytes().to_vec();
    not_utf8.extend([0xff, 0xfe, b'\n']);
    let inputs: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("blank", b" \n\t\r\n".to_vec()),
        ("two lines", format!("{INVOICE}\n{INVOICE}\n").into_bytes()),
        (
            "a line after a blank one",
            format!("{INVOICE}\n\nlnbc2\n").into_bytes(),
        ),
        ("more than 8 KiB", long.into_bytes()),
        ("invalid UTF-8", not_utf8),
    ];
    for kind in Kind::ALL {
        let world = world(kind).await;
        let before = world.snapshot();
        for (label, input) in &inputs {
            let label = format!("{}: {label}", kind.canonical());

            let json = world.piped(&["--output", "json"], &["--yes"], input).await;
            let human = world.piped(&[], &["--yes"], input).await;

            assert_eq!(json.code(), Some(2), "{label}: {}", json.stdout_text());
            let outcome = document(&json);
            assert_eq!(outcome["command"], kind.canonical(), "{label}");
            assert_eq!(outcome["error"]["code"], "invalid_arguments", "{label}");
            // The content of stdin is the invoice: it is refused as one.
            assert_eq!(outcome["error"]["reason"], "invalid_invoice", "{label}");
            assert_eq!(outcome["data"], Value::Null, "{label}");
            assert_eq!(
                outcome["meta"],
                json!({"publication": "not_attempted", "daemon_acceptance": "unknown",
                       "persistence": "not_attempted", "snapshot": null}),
                "{label}"
            );
            assert!(json.stderr.is_empty(), "{label}: {}", json.stderr_text());
            assert_eq!(human.code(), Some(2), "{label}");
            assert!(human.stdout.is_empty(), "{label}: {}", human.stdout_text());
            assert_eq!(
                human.stderr_text(),
                "error: The command arguments are invalid. (reason: invalid_invoice)\n",
                "{label}"
            );
            assert_never_shown(&json, INVOICE, &label);
            assert_never_shown(&human, INVOICE, &label);
        }
        assert!(
            world.fake.received().is_empty(),
            "{}: a request was sent",
            kind.canonical()
        );
        assert_eq!(
            world.snapshot(),
            before,
            "{}: the store changed",
            kind.canonical()
        );
    }
}

/// Without `--yes`, `--invoice -` is refused in both modes before stdin is
/// read: a child whose stdin stays open still ends at once.
#[tokio::test]
async fn without_yes_stdin_is_never_read() {
    for kind in Kind::ALL {
        let world = world(kind).await;
        let before = world.snapshot();
        for front in [&[][..], &["--output", "json"][..]] {
            let label = format!("{} {front:?}", kind.canonical());

            let run = run_with_open_stdin(world.command(front, "-", &[]), NO_WAIT)
                .await
                .unwrap();

            assert!(run.status.is_some(), "{label}: it waited for stdin");
            assert_eq!(run.code(), Some(2), "{label}");
            if front.is_empty() {
                assert!(run.stdout.is_empty(), "{label}: {}", run.stdout_text());
                assert_eq!(
                    run.stderr_text(),
                    "error: The command arguments are invalid.\n"
                );
            } else {
                assert_eq!(
                    document(&run)["error"]["code"],
                    "invalid_arguments",
                    "{label}"
                );
            }
        }
        assert!(world.fake.received().is_empty(), "{}", kind.canonical());
        assert_eq!(world.snapshot(), before, "{}", kind.canonical());
    }
}

/// A literal invoice keeps working, and keeps the human question when
/// `--yes` is absent (the answer comes from stdin, as before).
#[tokio::test]
async fn a_literal_invoice_is_unchanged() {
    let world = world(Kind::AddInvoice).await;
    let run = run_with_input(world.command(&[], INVOICE, &[]), b"y\n".to_vec(), WATCHDOG)
        .await
        .unwrap();
    assert_eq!(run.code(), Some(0), "{}", run.stderr_text());
    assert!(
        run.stdout_text().contains("Add Invoice Preview"),
        "{}",
        run.stdout_text()
    );
    assert_eq!(world.fake.received()[0].invoice.as_deref(), Some(INVOICE));
}

/// The checked-in output schema of `command`.
fn schema_of(command: &str) -> Value {
    let path = format!(
        "{}/docs/schemas/{}-v1.json",
        env!("CARGO_MANIFEST_DIR"),
        command.replace('.', "-")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// An invoice the client refuses, literal or from stdin, is `invalid_arguments`
/// with the reason `invalid_invoice`; every other argument error carries no
/// reason. The fixed message never contains the invoice.
#[tokio::test]
async fn an_invoice_the_client_refuses_carries_the_reason_invalid_invoice() {
    for kind in Kind::ALL {
        let world = world(kind).await;
        let before = world.snapshot();
        let label = kind.canonical();
        // A value the command's own check refuses: an address where only
        // BOLT-11 is accepted, text that is no invoice, an invoice with a space.
        let bad = match kind {
            Kind::BondInvoice => "buyer@example.SENTINEL.com",
            Kind::AddInvoice => "notaninvoiceSENTINEL",
            Kind::TakeSell | Kind::New => "lnbc1 SENTINEL",
        };
        let schema = schema_of(label);
        let mut refused = Vec::new();
        for front in [&["--output", "json"][..], &[]] {
            let mut command = world.command(front, bad, &["--yes"]);
            command.stdin(std::process::Stdio::null());
            refused.push(harness::run_with_watchdog(command, WATCHDOG).await.unwrap());
        }
        refused.push(world.piped(&["--output", "json"], &["--yes"], b"").await);

        for (index, run) in refused.iter().enumerate() {
            assert_eq!(
                run.code(),
                Some(2),
                "{label} #{index}: {}",
                run.stdout_text()
            );
            assert_never_shown(run, "SENTINEL", label);
            assert!(!run.stdout_text().contains("example"), "{label}");
            if run.stdout.is_empty() {
                assert_eq!(
                    run.stderr_text(),
                    "error: The command arguments are invalid. (reason: invalid_invoice)\n",
                    "{label}"
                );
                continue;
            }
            let outcome = document(run);
            assert!(
                schema_check::conforms(&schema, &schema, &outcome),
                "{label}: {outcome}"
            );
            assert_eq!(
                outcome["error"],
                json!({"code": "invalid_arguments", "message": "The command arguments are invalid.",
                       "reason": "invalid_invoice"}),
                "{label} #{index}"
            );
        }

        // Argument errors that are not about the invoice keep no reason.
        let mut others =
            vec![
                run_with_open_stdin(world.command(&["--output", "json"], "-", &[]), NO_WAIT)
                    .await
                    .unwrap(),
            ];
        let mut malformed = world.command(&["--output", "json"], &kind.invoice(), &["--yes"]);
        malformed.arg("--bogus-flag");
        others.push(
            harness::run_with_watchdog(malformed, WATCHDOG)
                .await
                .unwrap(),
        );
        if kind == Kind::New {
            let mut zero = world.command(&["--output", "json"], &kind.invoice(), &["--yes"]);
            zero.args(["--premium", "500"]);
            others.push(harness::run_with_watchdog(zero, WATCHDOG).await.unwrap());
        }
        for run in &others {
            assert_eq!(run.code(), Some(2), "{label}: {}", run.stdout_text());
            let outcome = document(run);
            assert!(
                schema_check::conforms(&schema, &schema, &outcome),
                "{label}: {outcome}"
            );
            assert_eq!(outcome["error"]["code"], "invalid_arguments", "{label}");
            assert!(
                outcome["error"].get("reason").is_none(),
                "{label}: {outcome}"
            );
        }
        assert!(
            world.fake.received().is_empty(),
            "{label}: a request was sent"
        );
        assert_eq!(world.snapshot(), before, "{label}: the store changed");
    }
}

/// The schemas allow the reason only where it can occur: a refusal by the node,
/// or `invalid_invoice` on `invalid_arguments`.
#[test]
fn the_schemas_tie_the_reason_to_its_code() {
    for command in [
        "order.add-invoice",
        "bond.add-invoice",
        "order.take-sell",
        "order.new",
    ] {
        let schema = schema_of(command);
        let error = |code: &str, reason: Option<&str>| {
            let mut error = json!({"code": code, "message": "x"});
            if let Some(reason) = reason {
                error["reason"] = json!(reason);
            }
            json!({"schema_version": "1", "command": command, "status": "error", "data": null,
                   "error": error, "warnings": [],
                   "meta": {"publication": "not_attempted", "daemon_acceptance": "unknown",
                            "persistence": "not_attempted", "snapshot": null}})
        };
        let accepts = |outcome: &Value| schema_check::conforms(&schema, &schema, outcome);
        assert!(
            accepts(&error("invalid_arguments", Some("invalid_invoice"))),
            "{command}"
        );
        assert!(accepts(&error("invalid_arguments", None)), "{command}");
        assert!(
            !accepts(&error("invalid_arguments", Some("invalid_amount"))),
            "{command}"
        );
        assert!(
            !accepts(&error("order_unknown", Some("invalid_invoice"))),
            "{command}"
        );
    }
    let take_buy = schema_of("order.take-buy");
    let outcome = json!({"schema_version": "1", "command": "order.take-buy", "status": "error",
        "data": null,
        "error": {"code": "invalid_arguments", "message": "x", "reason": "invalid_invoice"},
        "warnings": [],
        "meta": {"publication": "not_attempted", "daemon_acceptance": "unknown",
                 "persistence": "not_attempted", "snapshot": null}});
    assert!(
        !schema_check::conforms(&take_buy, &take_buy, &outcome),
        "take-buy takes no invoice"
    );
}
