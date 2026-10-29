use mostro_client::cli::new_order::{confirm_new_order, validate_market_fiat, FiatNames};
use mostro_client::cli::take_order::{accepted_take_local_save_failed, save_taken_sell_order};
use mostro_client::cli::Context;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use rstest::rstest;
use sqlx::SqlitePool;
use std::env::{current_exe, temp_dir, var, var_os};
use std::fs::{create_dir, read_dir, remove_dir_all};
use std::io::stdin;
use std::path::PathBuf;
use std::process::{exit, Stdio};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

struct PreflightSandbox(PathBuf);

impl Drop for PreflightSandbox {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.0);
    }
}

#[test]
fn new_order_preflight_child() {
    let Ok(scenario) = var("MOSTRO_TEST_PREFLIGHT") else {
        return;
    };
    let fiat_names = FiatNames::from([("USD".to_string(), "US Dollar".to_string())]);
    let result = match scenario.as_str() {
        "confirm" => confirm_new_order(stdin().lock()),
        "known-fiat" => validate_market_fiat("USD", &fiat_names),
        "unknown-fiat" => validate_market_fiat("UNKNOWN", &fiat_names),
        _ => panic!("unexpected preflight scenario"),
    };
    match result {
        Ok(()) => println!("PREFLIGHT_APPROVED"),
        Err(error) => {
            eprintln!("{error}");
            exit(1);
        }
    }
}

#[rstest]
#[case::closed_stdin("confirm", None, Some("confirmation"))]
#[case::empty_pipe("confirm", Some(""), Some("confirmation"))]
#[case::blank_enter("confirm", Some("\n"), None)]
#[case::windows_enter("confirm", Some("\r\n"), None)]
#[case::yes("confirm", Some("y\n"), None)]
#[case::uppercase_yes("confirm", Some("Y\n"), None)]
#[case::rejection("confirm", Some("n\n"), Some("cancelled"))]
#[case::uppercase_rejection("confirm", Some("N\n"), Some("cancelled"))]
#[case::invalid_answer("confirm", Some("maybe\n"), Some("confirmation"))]
#[case::known_fiat("known-fiat", None, None)]
#[case::unknown_fiat("unknown-fiat", None, Some("-a"))]
#[tokio::test]
async fn new_order_preflight_process(
    #[case] scenario: &str,
    #[case] input: Option<&str>,
    #[case] expected_error: Option<&str>,
) {
    let sandbox_path = temp_dir().join(format!("mostro-preflight-{}", Uuid::new_v4()));
    create_dir(&sandbox_path).unwrap();
    let sandbox = PreflightSandbox(sandbox_path);
    let mut command = Command::new(current_exe().unwrap());
    command
        .args(["--exact", "new_order_preflight_child", "--nocapture"])
        .env_clear()
        .env("MOSTRO_TEST_PREFLIGHT", scenario)
        .env("HOME", &sandbox.0)
        .env("USERPROFILE", &sandbox.0)
        .env("MCLI_DIR", &sandbox.0)
        .current_dir(&sandbox.0)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in ["SYSTEMROOT", "WINDIR"] {
        if let Some(value) = var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .await
            .unwrap();
    }
    if timeout(Duration::from_secs(40), child.wait())
        .await
        .is_err()
    {
        child.kill().await.unwrap();
        child.wait().await.unwrap();
        panic!("preflight child exceeded its deadline");
    }
    let output = child.wait_with_output().await.unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(read_dir(&sandbox.0).unwrap().next().is_none());
    assert_eq!(
        output.status.code(),
        Some(if expected_error.is_some() { 1 } else { 0 }),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(
        stdout.contains("PREFLIGHT_APPROVED"),
        expected_error.is_none()
    );
    if let Some(message) = expected_error {
        assert!(stderr.contains(message), "stderr: {stderr}");
    }
}

const TAKEN_SELL_ORDER: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";

fn taken_sell_order_id() -> Uuid {
    Uuid::parse_str(TAKEN_SELL_ORDER).unwrap()
}

async fn take_save_pool(last_trade_index: i64) -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE orders (
            id TEXT PRIMARY KEY, kind TEXT NOT NULL, status TEXT NOT NULL,
            amount INTEGER NOT NULL, min_amount INTEGER, max_amount INTEGER,
            fiat_code TEXT NOT NULL, fiat_amount INTEGER NOT NULL,
            payment_method TEXT NOT NULL, premium INTEGER NOT NULL,
            trade_keys TEXT, counterparty_pubkey TEXT, is_mine BOOLEAN,
            buyer_invoice TEXT, request_id INTEGER, created_at INTEGER,
            expires_at INTEGER, solver_pubkey TEXT
        );
        CREATE TABLE users (
            i0_pubkey char(64) PRIMARY KEY, mnemonic TEXT,
            last_trade_index INTEGER, created_at INTEGER
        );
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO users VALUES (?, 'test', ?, 0)")
        .bind("ab".repeat(32))
        .bind(last_trade_index)
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn offline_take_ctx(pool: SqlitePool, trade: &Keys, trade_index: i64) -> Context {
    Context {
        client: Client::builder()
            .authenticator(SignerAuthenticator::new(trade.clone()))
            .build(),
        identity_keys: trade.clone(),
        trade_keys: trade.clone(),
        trade_index,
        pool,
        context_keys: None,
        mostro_pubkey: Keys::generate().public_key(),
    }
}

#[tokio::test]
async fn takesell_post_accept_save_keeps_accepted_status_and_order_cause() {
    let pool = take_save_pool(3).await;
    sqlx::query(
        "CREATE TRIGGER refuse_orders BEFORE INSERT ON orders \
         BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
    )
    .execute(&pool)
    .await
    .unwrap();
    let order_id = taken_sell_order_id();
    let ctx = offline_take_ctx(pool, &Keys::generate(), 4);

    let error = accepted_take_local_save_failed(
        &order_id,
        save_taken_sell_order(&order_id, None, 77, &ctx)
            .await
            .expect_err("a refused order insert must not report success"),
    );
    let text = format!("{error:#}");

    assert!(
        text.contains(&format!("Mostro accepted the take of {order_id}")),
        "{text}"
    );
    assert!(text.contains("disk full"), "{text}");
    assert!(
        mostro_client::db::Order::get_by_id(&ctx.pool, TAKEN_SELL_ORDER)
            .await
            .is_err(),
        "no local row after a refused insert"
    );
}

#[tokio::test]
async fn takesell_post_accept_save_keeps_accepted_status_and_index_cause() {
    let pool = take_save_pool(3).await;
    sqlx::query("DROP TABLE users")
        .execute(&pool)
        .await
        .unwrap();
    let order_id = taken_sell_order_id();
    let ctx = offline_take_ctx(pool, &Keys::generate(), 4);

    let error = accepted_take_local_save_failed(
        &order_id,
        save_taken_sell_order(&order_id, None, 77, &ctx)
            .await
            .expect_err("a missing user row must not report success"),
    );
    let text = format!("{error:#}");

    assert!(
        text.contains(&format!("Mostro accepted the take of {order_id}")),
        "{text}"
    );
    assert!(
        text.contains("trade index") || text.contains("local user") || text.contains("user"),
        "{text}"
    );
}

// Test rate_user helper function
#[test]
fn test_get_user_rate_valid_ratings() {
    let valid_ratings = vec![1u8, 2u8, 3u8, 4u8, 5u8];
    for rating in valid_ratings {
        assert!((1..=5).contains(&rating));
    }
}

#[test]
fn test_invalid_ratings_out_of_range() {
    let invalid_ratings = vec![0u8, 6u8, 10u8, 255u8];
    for rating in invalid_ratings {
        assert!(!(1..=5).contains(&rating));
    }
}

#[test]
fn test_orders_info_empty_order_ids() {
    let order_ids: Vec<Uuid> = Vec::new();
    assert!(order_ids.is_empty());
}

#[test]
fn test_orders_info_single_order_id() {
    let order_id = Uuid::new_v4();
    let order_ids = [order_id];
    assert_eq!(order_ids.len(), 1);
    assert_eq!(order_ids[0], order_id);
}

#[test]
fn test_orders_info_multiple_order_ids() {
    let order_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    assert_eq!(order_ids.len(), 3);
    assert_ne!(order_ids[0], order_ids[1]);
    assert_ne!(order_ids[1], order_ids[2]);
    assert_ne!(order_ids[0], order_ids[2]);
}

#[test]
fn test_orders_info_payload_creation() {
    let order_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    let payload = Payload::Ids(order_ids.clone());
    match payload {
        Payload::Ids(ids) => {
            assert_eq!(ids.len(), 2);
            assert_eq!(ids, order_ids);
        }
        _ => panic!("Expected Payload::Ids"),
    }
}

#[test]
fn test_message_creation_for_orders_action() {
    let order_ids = vec![Uuid::new_v4()];
    let request_id = Uuid::new_v4().as_u128() as u64;
    let trade_index = 5i64;
    let payload = Payload::Ids(order_ids.clone());
    let message = Message::new_order(
        None,
        Some(request_id),
        Some(trade_index),
        Action::Orders,
        Some(payload),
    );
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Orders);
    assert_eq!(inner.request_id, Some(request_id));
    assert_eq!(inner.trade_index, Some(trade_index));
    assert!(inner.id.is_none());
}

#[test]
fn test_message_serialization_for_orders() {
    let order_ids = vec![Uuid::new_v4()];
    let payload = Payload::Ids(order_ids);
    let message = Message::new_order(None, Some(12345), Some(1), Action::Orders, Some(payload));
    let json_result = message.as_json();
    assert!(json_result.is_ok());
    let json_str = json_result.unwrap();
    assert!(!json_str.is_empty());
    assert!(json_str.contains("orders"));
}

#[test]
fn test_restore_message_creation() {
    let restore_message = Message::new_restore(None);
    let inner = restore_message.get_inner_message_kind();
    assert_eq!(inner.action, Action::RestoreSession);
    assert!(inner.payload.is_none());
}

#[test]
fn test_restore_message_serialization() {
    let restore_message = Message::new_restore(None);
    let json_result = restore_message.as_json();
    assert!(json_result.is_ok());
    let json_str = json_result.unwrap();
    assert!(!json_str.is_empty());
    assert!(json_str.contains("restore-session"));
}

#[test]
fn test_rating_payload_creation() {
    for rating in 1u8..=5u8 {
        let payload = Payload::RatingUser(rating);
        match payload {
            Payload::RatingUser(r) => {
                assert_eq!(r, rating);
                assert!((1..=5).contains(&r));
            }
            _ => panic!("Expected Payload::RatingUser"),
        }
    }
}

#[test]
fn test_rate_user_message_creation() {
    let order_id = Uuid::new_v4();
    let rating = 5u8;
    let payload = Payload::RatingUser(rating);
    let message = Message::new_order(Some(order_id), None, None, Action::RateUser, Some(payload));
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::RateUser);
    assert_eq!(inner.id, Some(order_id));
    match inner.payload {
        Some(Payload::RatingUser(r)) => assert_eq!(r, rating),
        _ => panic!("Expected RatingUser payload"),
    }
}

#[test]
fn test_take_buy_payload_with_amount() {
    let amount = 50000i64;
    let payload = Payload::Amount(amount);
    match payload {
        Payload::Amount(amt) => assert_eq!(amt, amount),
        _ => panic!("Expected Payload::Amount"),
    }
}

#[test]
fn test_take_sell_payload_with_invoice() {
    let invoice = "lnbc1000n1...".to_string();
    let payload = Payload::PaymentRequest(None, invoice.clone(), None);
    match payload {
        Payload::PaymentRequest(_, inv, _) => assert_eq!(inv, invoice),
        _ => panic!("Expected Payload::PaymentRequest"),
    }
}

#[test]
fn test_take_sell_payload_with_invoice_and_amount() {
    let invoice = "lnbc1000n1...".to_string();
    let amount = 75000i64;
    let payload = Payload::PaymentRequest(None, invoice.clone(), Some(amount));
    match payload {
        Payload::PaymentRequest(_, inv, Some(amt)) => {
            assert_eq!(inv, invoice);
            assert_eq!(amt, amount);
        }
        _ => panic!("Expected Payload::PaymentRequest with amount"),
    }
}

#[test]
fn test_dispute_message_creation_add_solver() {
    let dispute_id = Uuid::new_v4();
    let npubkey = "npub1...";
    let payload = Payload::TextMessage(npubkey.to_string());
    let message = Message::new_dispute(
        Some(dispute_id),
        None,
        None,
        Action::AdminAddSolver,
        Some(payload),
    );
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::AdminAddSolver);
    assert_eq!(inner.id, Some(dispute_id));
}

#[test]
fn test_dispute_message_cancel() {
    let dispute_id = Uuid::new_v4();
    let message = Message::new_dispute(Some(dispute_id), None, None, Action::AdminCancel, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::AdminCancel);
    assert_eq!(inner.id, Some(dispute_id));
}

#[test]
fn test_dispute_message_settle() {
    let dispute_id = Uuid::new_v4();
    let message = Message::new_dispute(Some(dispute_id), None, None, Action::AdminSettle, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::AdminSettle);
    assert_eq!(inner.id, Some(dispute_id));
}

#[test]
fn test_dispute_message_take() {
    let dispute_id = Uuid::new_v4();
    let message =
        Message::new_dispute(Some(dispute_id), None, None, Action::AdminTakeDispute, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::AdminTakeDispute);
    assert_eq!(inner.id, Some(dispute_id));
}

#[test]
fn test_new_order_message_with_trade_index() {
    let trade_index = 42i64;
    let payload = Payload::Order(SmallOrder {
        id: None,
        kind: Some(mostro_core::order::Kind::Buy),
        status: Some(Status::Pending),
        amount: 100000,
        fiat_code: "USD".to_string(),
        min_amount: None,
        max_amount: None,
        fiat_amount: 1000,
        payment_method: "cash".to_string(),
        premium: 0,
        buyer_trade_pubkey: None,
        seller_trade_pubkey: None,
        buyer_invoice: None,
        created_at: None,
        expires_at: None,
        cashu_mint_url: None,
    });
    let message = Message::new_order(
        None,
        None,
        Some(trade_index),
        Action::NewOrder,
        Some(payload),
    );
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::NewOrder);
    assert_eq!(inner.trade_index, Some(trade_index));
}

#[test]
fn test_send_msg_cancel_action() {
    let order_id = Uuid::new_v4();
    let message = Message::new_order(Some(order_id), None, None, Action::Cancel, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Cancel);
    assert_eq!(inner.id, Some(order_id));
}

#[test]
fn test_send_msg_fiat_sent_action() {
    let order_id = Uuid::new_v4();
    let message = Message::new_order(Some(order_id), None, None, Action::FiatSent, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::FiatSent);
    assert_eq!(inner.id, Some(order_id));
}

#[test]
fn test_send_msg_release_action() {
    let order_id = Uuid::new_v4();
    let message = Message::new_order(Some(order_id), None, None, Action::Release, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Release);
    assert_eq!(inner.id, Some(order_id));
}

#[test]
fn test_send_msg_dispute_action() {
    let order_id = Uuid::new_v4();
    let message = Message::new_dispute(Some(order_id), None, None, Action::Dispute, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::Dispute);
    assert_eq!(inner.id, Some(order_id));
}

#[test]
fn test_dm_message_creation() {
    let message_text = "Hello, how are you?";
    let payload = Payload::TextMessage(message_text.to_string());
    let message = Message::new_dm(None, None, Action::SendDm, Some(payload));
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::SendDm);
    assert!(inner.id.is_none());
    match &inner.payload {
        Some(Payload::TextMessage(text)) => assert_eq!(text, message_text),
        _ => panic!("Expected TextMessage payload"),
    }
}

#[test]
fn test_last_trade_index_message() {
    let message = Message::new_order(None, None, None, Action::LastTradeIndex, None);
    let inner = message.get_inner_message_kind();
    assert_eq!(inner.action, Action::LastTradeIndex);
    assert!(inner.id.is_none());
    assert!(inner.payload.is_none());
}

// ── admin maintenance-mode commands (gRPC, no Nostr context) ──────────────
mod maintenance_commands {
    use clap::Parser;
    use mostro_client::cli::{Cli, Commands};

    #[test]
    fn admsetmaintenance_parses_explicit_bool_and_reason() {
        let cli = Cli::try_parse_from([
            "mostro-cli",
            "admsetmaintenance",
            "--enabled",
            "true",
            "--reason",
            "LN node migration",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::AdmSetMaintenance { enabled, reason }) => {
                assert!(enabled);
                assert_eq!(reason.as_deref(), Some("LN node migration"));
            }
            _ => panic!("unexpected command"),
        }

        let cli = Cli::try_parse_from(["mostro-cli", "admsetmaintenance", "-e", "false"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::AdmSetMaintenance {
                enabled: false,
                reason: None
            })
        ));
    }

    #[test]
    fn admsetmaintenance_requires_the_enabled_value() {
        assert!(Cli::try_parse_from(["mostro-cli", "admsetmaintenance"]).is_err());
        assert!(Cli::try_parse_from(["mostro-cli", "admsetmaintenance", "--enabled"]).is_err());
        assert!(
            Cli::try_parse_from(["mostro-cli", "admsetmaintenance", "--enabled", "yes"]).is_err()
        );
    }

    #[test]
    fn admmaintenancestatus_parses_without_arguments() {
        let cli = Cli::try_parse_from(["mostro-cli", "admmaintenancestatus"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::AdmMaintenanceStatus {})
        ));
    }

    /// The gRPC commands must be dispatched before any Nostr context (relays,
    /// mnemonic, ADMIN_NSEC) is built; Nostr commands must not be.
    #[tokio::test]
    async fn rpc_commands_are_routed_before_the_nostr_context() {
        let cli = Cli::try_parse_from(["mostro-cli", "listdisputes"]).unwrap();
        assert!(cli.command.unwrap().run_rpc().await.is_none());

        // Point at a closed port so the attempt fails fast instead of hanging.
        std::env::set_var("MOSTRO_RPC_URL", "http://127.0.0.1:1");
        let cli = Cli::try_parse_from(["mostro-cli", "admmaintenancestatus"]).unwrap();
        let result = cli.command.unwrap().run_rpc().await;
        let err = result.expect("rpc command must be handled").unwrap_err();
        assert!(
            err.to_string().contains("cannot reach mostrod admin RPC"),
            "{err}"
        );
    }
}

// ── output-intent recovery on malformed argv (contracts/cli.md) ────────────
mod output_intent {
    use clap::error::ErrorKind;
    use clap::{ArgMatches, Command, CommandFactory};
    use mostro_client::cli::invocation::{
        parse_invocation, ArgumentFailure, Invocation, OutputMode, OutputSelection, OUTPUT_LONG,
    };
    use mostro_client::cli::Cli;
    use mostro_client::outcome::order::{SIDES, STATUSES};
    use mostro_client::outcome::ErrorCode;
    use std::ffi::OsString;

    const PUBKEY: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const ORDER: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";

    /// Expected handling of one rejected invocation.
    #[derive(Debug)]
    enum Expect {
        /// JSON invalid_arguments envelope naming this leaf, or command null.
        Json(Option<&'static [&'static str]>),
        /// Human-mode error on stderr.
        Human,
        /// Missing, invalid or duplicate output: empty stdout, safe stderr.
        InvalidSelection,
    }

    /// The real `Cli`, including its `order list` grammar.
    fn pilot_grammar() -> Command {
        Cli::command()
    }

    fn invoke_os(suffix: Vec<OsString>) -> Invocation {
        let argv = std::iter::once(OsString::from("mostro-cli")).chain(suffix);
        parse_invocation(pilot_grammar(), argv)
    }

    fn invoke(suffix: &[&str]) -> Invocation {
        invoke_os(suffix.iter().map(OsString::from).collect())
    }

    fn assert_rejected(invocation: Invocation, expect: &Expect, label: &str) {
        let Invocation::Rejected(failure) = invocation else {
            panic!("{label}: expected an argument error, got {invocation:?}");
        };
        let expected_output = match expect {
            Expect::Json(_) => OutputSelection::Mode(OutputMode::Json),
            Expect::Human => OutputSelection::Mode(OutputMode::Human),
            Expect::InvalidSelection => OutputSelection::Invalid,
        };
        assert_eq!(failure.output, expected_output, "{label}");
        match expect {
            Expect::Json(command) => {
                let command = command.map(|path| path.iter().map(|s| s.to_string()).collect());
                assert_eq!(failure.command, command, "{label}");
                assert_eq!(
                    failure.json_error_code(),
                    Some(ErrorCode::InvalidArguments),
                    "{label}"
                );
            }
            Expect::Human | Expect::InvalidSelection => {
                assert_eq!(failure.json_error_code(), None, "{label}");
            }
        }
    }

    fn check(rows: &[(&[&str], Expect)]) {
        for (suffix, expect) in rows {
            assert_rejected(invoke(suffix), expect, &format!("{suffix:?}"));
        }
    }

    /// The output mode Clap itself bound, searching the root and every
    /// subcommand level, plus the subcommand path.
    fn clap_view(matches: &ArgMatches) -> (Option<OutputMode>, Vec<String>) {
        let mut output = matches.get_one::<OutputMode>(OUTPUT_LONG).copied();
        let mut path = Vec::new();
        let mut level = matches;
        while let Some((name, sub)) = level.subcommand() {
            path.push(name.to_string());
            level = sub;
            output = output.or(level.get_one::<OutputMode>(OUTPUT_LONG).copied());
        }
        (output, path)
    }

    /// A successful parse: the recovered mode must equal Clap's own value.
    fn parsed(suffix: &[&str]) -> (OutputMode, Vec<String>) {
        match invoke(suffix) {
            Invocation::Parsed { matches, output } => {
                let (bound, path) = clap_view(&matches);
                assert_eq!(output, bound.unwrap_or(OutputMode::Human), "{suffix:?}");
                (output, path)
            }
            other => panic!("{suffix:?}: expected a successful parse, got {other:?}"),
        }
    }

    fn text_kind(suffix: &[&str]) -> ErrorKind {
        match invoke(suffix) {
            Invocation::Text(error) => error.kind(),
            other => panic!("{suffix:?}: expected help/version text, got {other:?}"),
        }
    }

    const ORDER_LIST: Option<&[&str]> = Some(&["order", "list"]);

    #[test]
    fn every_side_and_status_spelling_parses_and_removed_spellings_fail() {
        let order_list = vec!["order".to_string(), "list".to_string()];
        for value in SIDES {
            for spelling in [value.to_string(), value.to_uppercase()] {
                let suffix = ["order", "list", "--side", spelling.as_str()];
                assert_eq!(parsed(&suffix), (OutputMode::Human, order_list.clone()));
            }
        }
        for value in STATUSES {
            for spelling in [value.to_string(), value.to_uppercase()] {
                let suffix = ["order", "list", "--status", spelling.as_str()];
                assert_eq!(parsed(&suffix), (OutputMode::Human, order_list.clone()));
            }
        }
        check(&[
            (&["order", "list", "--side", " sell"], Expect::Human),
            (&["order", "list", "--status", "all"], Expect::Human),
            (&["order", "list", "--kind", "sell"], Expect::Human),
            (&["listorders"], Expect::Human),
            (&["--mostropubkey", PUBKEY, "order", "list"], Expect::Human),
            (&["--output", "json", "listorders"], Expect::Json(None)),
            (
                &["--output", "json", "order", "list", "--kind", "sell"],
                Expect::Json(ORDER_LIST),
            ),
        ]);
        let suffix = ["--mostro-pubkey", PUBKEY, "order", "list"];
        assert_eq!(parsed(&suffix), (OutputMode::Human, order_list));
    }

    #[test]
    fn contract_decision_table_rows() {
        check(&[
            (
                &["--output", "json", "order", "list", "--side", "invalid"],
                Expect::Json(ORDER_LIST),
            ),
            (
                &["order", "--output=json", "list", "--status", "invalid"],
                Expect::Json(ORDER_LIST),
            ),
            (
                &["order", "list", "--output", "json", "--unknown"],
                Expect::Json(ORDER_LIST),
            ),
            (&["--output", "json", "unknown"], Expect::Json(None)),
            (
                &["order", "list", "--output", "unknown"],
                Expect::InvalidSelection,
            ),
            (&["order", "list", "--output"], Expect::InvalidSelection),
            (
                &["--output", "json", "order", "list", "--output=json"],
                Expect::InvalidSelection,
            ),
            (
                &["--output", "json", "order", "list", "--output", "human"],
                Expect::InvalidSelection,
            ),
            (
                &[
                    "order",
                    "list",
                    "--currency=--output=json",
                    "--side",
                    "invalid",
                ],
                Expect::Human,
            ),
            (&["order", "list", "--", "--output", "json"], Expect::Human),
            (
                &[
                    "--output", "json", "order", "list", "--", "--output", "human",
                ],
                Expect::Json(ORDER_LIST),
            ),
        ]);
        assert_eq!(
            text_kind(&["--output", "json", "--help"]),
            ErrorKind::DisplayHelp
        );
    }

    #[test]
    fn output_is_selected_at_every_depth_and_defaults_to_human() {
        let order_list = vec!["order".to_string(), "list".to_string()];
        for suffix in [
            &["--output", "json", "order", "list"][..],
            &["order", "--output=json", "list"],
            &["order", "list", "--output", "json"],
            &["order", "list", "--output=json"],
        ] {
            assert_eq!(parsed(suffix), (OutputMode::Json, order_list.clone()));
        }
        assert_eq!(
            parsed(&["order", "list"]),
            (OutputMode::Human, order_list.clone())
        );
        assert_eq!(
            parsed(&["order", "list", "--output", "human"]),
            (OutputMode::Human, order_list)
        );
        let legacy = vec!["listdisputes".to_string()];
        assert_eq!(
            parsed(&["listdisputes"]),
            (OutputMode::Human, legacy.clone())
        );
        assert_eq!(
            parsed(&["--output", "json", "listdisputes"]),
            (OutputMode::Json, legacy)
        );
        // Case-sensitive: only the lowercase spellings select a mode.
        check(&[(
            &["--output", "JSON", "order", "list"],
            Expect::InvalidSelection,
        )]);
    }

    #[test]
    fn values_resembling_flags_or_commands_are_not_declarations() {
        let senddm = vec!["senddm".to_string()];
        let base = ["senddm", "-p", PUBKEY, "-o", ORDER];
        let with = |tail: &[&'static str]| -> Vec<&'static str> {
            base.iter().copied().chain(tail.iter().copied()).collect()
        };
        assert_eq!(
            parsed(&with(&["-m=--output=json"])),
            (OutputMode::Human, senddm.clone())
        );
        assert_eq!(
            parsed(&with(&["--message=--output=json"])),
            (OutputMode::Human, senddm.clone())
        );
        assert_eq!(
            parsed(&with(&["-m", "order", "list"])),
            (OutputMode::Human, senddm.clone())
        );
        assert_eq!(
            parsed(&with(&["-m", "order", "list", "--output", "json"])),
            (OutputMode::Json, senddm)
        );
        check(&[
            // The root `-m` consumes `order`, so `list` is not a command.
            (
                &["--output", "json", "-m", "order", "list"],
                Expect::Json(None),
            ),
            (
                &["--output", "json", "--relays=order", "list"],
                Expect::Json(None),
            ),
            // A flag-looking token is not a value, so this is a declaration.
            (
                &with(&["-m", "--output=json"]),
                Expect::Json(Some(&["senddm"])),
            ),
        ]);
    }

    #[test]
    fn arities_and_hyphen_values_follow_clap() {
        let neworder = [
            "neworder", "-k", "sell", "-c", "USD", "-f", "100", "-m", "SEPA",
        ];
        let with = |tail: &[&'static str]| -> Vec<&'static str> {
            neworder
                .iter()
                .copied()
                .chain(tail.iter().copied())
                .collect()
        };
        assert_eq!(
            parsed(&with(&["-p", "-5", "--output", "json"])),
            (OutputMode::Json, vec!["neworder".to_string()])
        );
        check(&[
            // `--premium` accepts hyphen values, so it swallows `--output`.
            (&with(&["-p", "--output", "json"]), Expect::Human),
            // `--currency` does not, so its value is missing and the
            // declaration stands.
            (
                &["order", "list", "--currency", "--output", "json"],
                Expect::Json(ORDER_LIST),
            ),
            // Root `--pow` is missing its value; the later declaration stands.
            (
                &["-p", "--output", "json", "order", "list"],
                Expect::Json(ORDER_LIST),
            ),
            (
                &["--output", "json", "order", "list", "--side"],
                Expect::Json(ORDER_LIST),
            ),
        ]);
    }

    #[test]
    fn short_clusters_and_colliding_shorts_follow_scope() {
        let order_list = vec!["order".to_string(), "list".to_string()];
        assert_eq!(
            parsed(&["-vs", "--output", "json", "order", "list"]),
            (OutputMode::Json, order_list.clone())
        );
        assert_eq!(
            parsed(&["-vm", PUBKEY, "order", "list", "--output", "json"]),
            (OutputMode::Json, order_list.clone())
        );
        // `-m` takes the rest of the cluster as its value.
        assert_eq!(
            parsed(&["-vm--output=json", "order", "list"]),
            (OutputMode::Human, order_list.clone())
        );
        assert_eq!(
            parsed(&["-m=--output=json", "order", "list"]),
            (OutputMode::Human, order_list)
        );
        // Leaf `-m` is the message, not the root pubkey.
        assert_eq!(
            parsed(&["senddm", "-p", PUBKEY, "-o", ORDER, "-mhello", "--output", "json"]),
            (OutputMode::Json, vec!["senddm".to_string()])
        );
        check(&[(
            &["-vx", "--output", "json", "order", "list"],
            Expect::Json(ORDER_LIST),
        )]);
    }

    #[test]
    fn duplicate_and_missing_declarations_are_invalid() {
        check(&[
            (
                &["--output", "json", "--output", "json", "order", "list"],
                Expect::InvalidSelection,
            ),
            (
                &["order", "list", "--output=human", "--output=human"],
                Expect::InvalidSelection,
            ),
            (
                &["--output", "json", "order", "--output", "json", "list"],
                Expect::InvalidSelection,
            ),
            (&["--output"], Expect::InvalidSelection),
            (&["order", "list", "--output="], Expect::InvalidSelection),
            (
                &["order", "list", "--output", "--side", "buy"],
                Expect::InvalidSelection,
            ),
            (
                &["order", "list", "--output", "--", "json"],
                Expect::InvalidSelection,
            ),
        ]);
    }

    #[test]
    fn terminator_ends_option_parsing() {
        check(&[
            (
                &[
                    "senddm", "-p", PUBKEY, "-o", ORDER, "-m", "a", "--", "--output", "json",
                ],
                Expect::Human,
            ),
            (
                &["--output", "json", "--", "order", "list"],
                Expect::Json(None),
            ),
        ]);
    }

    #[test]
    fn clap_decides_help_and_version_precedence() {
        assert_eq!(
            text_kind(&["--output", "json", "--help"]),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            text_kind(&["order", "list", "--output", "json", "--help"]),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            text_kind(&["order", "list", "--help", "--output", "json"]),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            text_kind(&["help", "order", "list"]),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            text_kind(&["--output", "json", "--version"]),
            ErrorKind::DisplayVersion
        );
        check(&[
            // Clap reports the invalid value before it reaches `--help`.
            (&["--output", "bogus", "--help"], Expect::InvalidSelection),
            // A missing subcommand shows help as an error, not as a request.
            (&["--output", "json", "order"], Expect::Json(None)),
        ]);
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_values_keep_json_intent() {
        use std::os::unix::ffi::OsStringExt;
        let invalid = || OsString::from_vec(vec![b'U', 0xff, b'D']);
        let args = |items: &[&str]| -> Vec<OsString> { items.iter().map(OsString::from).collect() };

        let mut suffix = args(&["order", "list", "--currency"]);
        suffix.push(invalid());
        suffix.extend(args(&["--output", "json"]));
        assert_rejected(
            invoke_os(suffix),
            &Expect::Json(ORDER_LIST),
            "--currency <bytes>",
        );

        let mut attached = OsString::from("--currency=");
        attached.push(invalid());
        let mut suffix = args(&["--output", "json", "order", "list"]);
        suffix.push(attached);
        assert_rejected(
            invoke_os(suffix),
            &Expect::Json(ORDER_LIST),
            "--currency=<bytes>",
        );

        let mut cluster = OsString::from("-m");
        cluster.push(invalid());
        let mut suffix = vec![cluster];
        suffix.extend(args(&["--output", "json", "order", "list"]));
        assert_rejected(invoke_os(suffix), &Expect::Json(ORDER_LIST), "-m<bytes>");

        let mut suffix = args(&["order", "list", "--output"]);
        suffix.push(invalid());
        assert_rejected(
            invoke_os(suffix),
            &Expect::InvalidSelection,
            "--output <bytes>",
        );
    }

    #[test]
    fn failures_carry_no_argument_text() {
        let failure = ArgumentFailure {
            output: OutputSelection::Mode(OutputMode::Json),
            command: None,
            kind: ErrorKind::InvalidValue,
        };
        assert_eq!(failure.json_error_code(), Some(ErrorCode::InvalidArguments));
        let Invocation::Rejected(failure) = invoke(&[
            "--output",
            "json",
            "order",
            "list",
            "--currency",
            "--side",
            "secret-sentinel",
        ]) else {
            panic!("expected an argument error");
        };
        assert!(!format!("{failure:?}").contains("secret-sentinel"));
    }

    fn invoke_real(suffix: &[&str]) -> Invocation {
        let argv = std::iter::once("mostro-cli").chain(suffix.iter().copied());
        parse_invocation(Cli::command(), argv)
    }

    #[test]
    fn real_cli_declares_a_global_case_sensitive_output() {
        let mut command = Cli::command();
        command.build();
        let output = command
            .get_arguments()
            .find(|arg| arg.get_id() == OUTPUT_LONG)
            .expect("--output is declared");
        assert_eq!(output.get_long(), Some(OUTPUT_LONG));
        assert_eq!(output.get_short(), None);
        assert!(output.is_global_set());
        assert!(!output.is_ignore_case_set());
        let values: Vec<String> = output
            .get_possible_values()
            .iter()
            .map(|value| value.get_name().to_string())
            .collect();
        assert_eq!(values, ["human", "json"]);
        // Clap's generated `help` subcommand takes command names only.
        for subcommand in command
            .get_subcommands()
            .filter(|sub| sub.get_name() != "help")
        {
            assert!(
                subcommand
                    .get_arguments()
                    .any(|arg| arg.get_id() == OUTPUT_LONG),
                "{} lacks --output",
                subcommand.get_name()
            );
        }

        let rows: [(&[&str], OutputMode); 4] = [
            (&["order", "list"], OutputMode::Human),
            (&["order", "list", "--output", "json"], OutputMode::Json),
            (&["--output=json", "order", "list"], OutputMode::Json),
            (&["--output", "human", "order", "list"], OutputMode::Human),
        ];
        for (suffix, mode) in rows {
            let Invocation::Parsed { matches, output } = invoke_real(suffix) else {
                panic!("{suffix:?} did not parse");
            };
            assert_eq!(output, mode, "{suffix:?}");
            assert_eq!(clap_view(&matches).0.unwrap_or(OutputMode::Human), mode);
        }
        let Invocation::Rejected(failure) = invoke_real(&["--output", "JSON", "order", "list"])
        else {
            panic!("--output is case-sensitive");
        };
        assert_eq!(failure.output, OutputSelection::Invalid);
    }

    const SCHEMA: Option<&[&str]> = Some(&["schema"]);

    /// A valid invocation of a registered legacy command parses for JSON; the
    /// capability guard, not the parser, then answers output_not_supported.
    #[test]
    fn valid_unsupported_invocations_parse_and_malformed_ones_are_invalid_arguments() {
        let valid: [&[&str]; 5] = [
            &["getlasttradeprivkey"],
            &["getlasttradeindex"],
            &["restore"],
            &["admmaintenancestatus"],
            &["takesell", "--orderid", ORDER],
        ];
        for suffix in valid {
            let argv = [&["--output", "json"][..], suffix].concat();
            let (mode, path) = parsed(&argv);
            assert_eq!(mode, OutputMode::Json, "{argv:?}");
            let entry = mostro_client::cli::lookup(&path).expect("registered leaf");
            assert_eq!(entry.json, mostro_client::cli::JsonSupport::Unsupported);
        }
        check(&[
            (
                &["--output", "json", "takesell"],
                Expect::Json(Some(&["takesell"])),
            ),
            (
                &["--output", "json", "takesell", "--orderid", "not-a-uuid"],
                Expect::Json(Some(&["takesell"])),
            ),
            (
                &["--output", "json", "getlasttradeindex", "--bogus"],
                Expect::Json(Some(&["getlasttradeindex"])),
            ),
            (&["--output", "json", "order", "nosuch"], Expect::Json(None)),
            (
                &["--output", "json", "nosuch", "--bogus"],
                Expect::Json(None),
            ),
            (&["takesell"], Expect::Human),
        ]);
    }

    #[test]
    fn schema_parses_with_output_at_every_depth() {
        let schema = vec!["schema".to_string()];
        let rows: [(&[&str], OutputMode); 6] = [
            (&["schema"], OutputMode::Human),
            (&["schema", "--output", "json"], OutputMode::Json),
            (&["schema", "--output=json"], OutputMode::Json),
            (&["--output", "json", "schema"], OutputMode::Json),
            (&["--output=json", "schema"], OutputMode::Json),
            (&["--output", "human", "schema"], OutputMode::Human),
        ];
        for (suffix, mode) in rows {
            assert_eq!(parsed(suffix), (mode, schema.clone()), "{suffix:?}");
        }
        // The documented node options precede the command and are optional.
        let suffix = [
            "--mostro-pubkey",
            PUBKEY,
            "--relays",
            "ws://127.0.0.1:1",
            "schema",
        ];
        assert_eq!(parsed(&suffix).1, schema);
    }

    #[test]
    fn schema_takes_no_arguments_and_keeps_json_intent_on_errors() {
        check(&[
            (
                &["--output", "json", "schema", "extra"],
                Expect::Json(SCHEMA),
            ),
            (
                &["schema", "--output=json", "--bogus"],
                Expect::Json(SCHEMA),
            ),
            (
                &["--output", "json", "schema", "--side", "sell"],
                Expect::Json(SCHEMA),
            ),
            (&["schema", "extra"], Expect::Human),
            (&["schema", "--output"], Expect::InvalidSelection),
            (
                &["--output", "json", "schema", "--output", "json"],
                Expect::InvalidSelection,
            ),
            (&["schema", "--", "--output", "json"], Expect::Human),
        ]);
    }

    #[test]
    fn schema_help_and_version_stay_text() {
        assert_eq!(text_kind(&["schema", "--help"]), ErrorKind::DisplayHelp);
        assert_eq!(
            text_kind(&["--output", "json", "schema", "--help"]),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            text_kind(&["schema", "--output", "json", "--help"]),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            text_kind(&["--output", "json", "--version"]),
            ErrorKind::DisplayVersion
        );
        assert_eq!(text_kind(&["help", "schema"]), ErrorKind::DisplayHelp);
    }

    #[test]
    fn rejections_keep_the_clap_kind_and_a_safe_diagnostic() {
        let rows: [(&[&str], ErrorKind, &str); 4] = [
            (
                &["order", "list", "--secret-sentinel"],
                ErrorKind::UnknownArgument,
                "unexpected argument found",
            ),
            (
                &["secret-sentinel"],
                ErrorKind::InvalidSubcommand,
                "unrecognized subcommand",
            ),
            (
                &["--output", "secret-sentinel", "order", "list"],
                ErrorKind::InvalidValue,
                "--output",
            ),
            (
                &["--output", "json", "--output", "json", "order", "list"],
                ErrorKind::ArgumentConflict,
                "--output",
            ),
        ];
        for (suffix, kind, mentions) in rows {
            let Invocation::Rejected(failure) = invoke_real(suffix) else {
                panic!("{suffix:?} was accepted");
            };
            assert_eq!(failure.kind, kind, "{suffix:?}");
            let text = failure.diagnostic();
            assert!(text.starts_with("error: "), "{suffix:?}: {text:?}");
            assert!(text.contains(mentions), "{suffix:?}: {text:?}");
            assert!(!text.contains("secret-sentinel"), "{suffix:?}: {text:?}");
        }
        let Invocation::Rejected(bare) = invoke_real(&[]) else {
            panic!("a bare invocation is an argument error");
        };
        assert_eq!(
            bare.kind,
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }
}

mod registry {
    use clap::{Command, CommandFactory};
    use mostro_client::cli::{lookup, Cli, JsonSupport, REGISTRY};
    use mostro_client::outcome::{DaemonAcceptance, ExecutionMeta, Publication};
    use std::collections::BTreeSet;

    /// Canonical names approved on 2026-10-22, in subcommand order. T030
    /// replaced `listorders` with `order list`, keeping `order.list`.
    const APPROVED: [(&[&str], &str); 34] = [
        (&["order", "list"], "order.list"),
        (&["schema"], "schema"),
        (&["neworder"], "order.new"),
        (&["takesell"], "order.take-sell"),
        (&["takebuy"], "order.take-buy"),
        (&["addinvoice"], "order.add-invoice"),
        (&["addbondinvoice"], "bond.add-invoice"),
        (&["getdm"], "message.list"),
        (&["getdmuser"], "chat.history"),
        (&["disputechat"], "dispute.chat"),
        (&["getadmindm"], "admin.message-list"),
        (&["senddm"], "message.send"),
        (&["dmtouser"], "chat.send"),
        (&["fiatsent"], "order.fiat-sent"),
        (&["release"], "order.release"),
        (&["cancel"], "order.cancel"),
        (&["rate"], "order.rate"),
        (&["restore"], "account.restore"),
        (&["dispute"], "dispute.open"),
        (&["admcancel"], "admin.cancel"),
        (&["admsettle"], "admin.settle"),
        (&["listdisputes"], "dispute.list"),
        (&["admsetmaintenance"], "maintenance.set"),
        (&["admmaintenancestatus"], "maintenance.status"),
        (&["admcancelpending"], "maintenance.cancel-pending"),
        (&["admaddsolver"], "admin.add-solver"),
        (&["admtakedispute"], "admin.take-dispute"),
        (&["admsenddm"], "admin.message-send"),
        (&["sendadmindmattach"], "admin.message-attach"),
        (&["conversationkey"], "account.conversation-key"),
        (&["getlasttradeindex"], "account.trade-index"),
        (&["getlasttradeprivkey"], "account.trade-key"),
        (&["getuserinfo"], "account.info"),
        (&["order", "info"], "order.info"),
    ];

    fn path(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    /// Every executable leaf path, skipping Clap's generated `help`.
    fn leaves(command: &Command, prefix: &[String], out: &mut BTreeSet<Vec<String>>) {
        for sub in command.get_subcommands().filter(|s| s.get_name() != "help") {
            let mut here = prefix.to_vec();
            here.push(sub.get_name().to_string());
            if sub.has_subcommands() {
                leaves(sub, &here, out);
            } else {
                out.insert(here);
            }
        }
    }

    #[test]
    fn registry_matches_the_approved_mapping() {
        let actual: Vec<(&[&str], &str)> = REGISTRY
            .iter()
            .map(|entry| (entry.path, entry.canonical))
            .collect();
        assert_eq!(actual, APPROVED);
    }

    #[test]
    fn every_subcommand_is_registered_exactly_once() {
        let mut subcommands = BTreeSet::new();
        leaves(&Cli::command(), &[], &mut subcommands);
        let registered: BTreeSet<Vec<String>> =
            REGISTRY.iter().map(|entry| path(entry.path)).collect();
        assert_eq!(registered, subcommands);
        assert_eq!(registered.len(), REGISTRY.len());
        let canonical: BTreeSet<&str> = REGISTRY.iter().map(|entry| entry.canonical).collect();
        assert_eq!(
            canonical.len(),
            REGISTRY.len(),
            "canonical names must be unique"
        );
    }

    #[test]
    fn canonical_names_are_dotted_lowercase() {
        let lowercase =
            |word: &str| !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase());
        for entry in REGISTRY.iter().filter(|entry| entry.canonical != "schema") {
            let (noun, verb) = entry.canonical.split_once('.').expect("one dot");
            assert!(lowercase(noun), "{}", entry.canonical);
            assert!(verb.split('-').all(lowercase), "{}", entry.canonical);
        }
    }

    #[test]
    fn only_the_migrated_commands_support_json() {
        for entry in REGISTRY {
            match entry.canonical {
                "order.list" | "schema" => {
                    assert_eq!(entry.json, JsonSupport::Read);
                    assert_eq!(entry.stages::<()>(), ExecutionMeta::read(None));
                }
                "order.info" => {
                    assert_eq!(entry.json, JsonSupport::Request);
                    assert_eq!(
                        entry.stages::<()>(),
                        ExecutionMeta::request(
                            Publication::NotAttempted,
                            DaemonAcceptance::Unknown
                        )
                    );
                }
                other => {
                    assert_eq!(entry.json, JsonSupport::Unsupported, "{other}");
                    assert_eq!(entry.stages::<()>(), ExecutionMeta::not_executed());
                }
            }
        }
    }

    #[test]
    fn lookup_resolves_only_registered_leaves() {
        assert_eq!(
            lookup(&path(&["neworder"])).map(|e| e.canonical),
            Some("order.new")
        );
        assert_eq!(
            lookup(&path(&["order", "list"])).map(|e| e.canonical),
            Some("order.list")
        );
        assert_eq!(
            lookup(&path(&["getlasttradeprivkey"])).map(|e| e.canonical),
            Some("account.trade-key")
        );
        for unresolved in [
            &[][..],
            &["nosuch"],
            &["help"],
            &["order"],
            &["listorders"],
            &["list"],
            &["order", "list", "extra"],
        ] {
            assert_eq!(lookup(&path(unresolved)), None, "{unresolved:?}");
        }
    }

    #[test]
    fn schema_is_a_registered_machine_command_with_a_non_dotted_name() {
        let entry = lookup(&path(&["schema"])).expect("schema is registered");
        assert_eq!(entry.canonical, "schema");
        assert_ne!(entry.json, JsonSupport::Unsupported);
        assert_eq!(entry.stages::<()>(), ExecutionMeta::read(None));
        let machine: Vec<&str> = REGISTRY
            .iter()
            .filter(|entry| entry.json != JsonSupport::Unsupported)
            .map(|entry| entry.canonical)
            .collect();
        assert_eq!(machine, ["order.list", "schema", "order.info"]);
    }
}

/// The grammar of `order info`, the command that replaces `ordersinfo`.
mod order_info_grammar {
    use clap::Parser;
    use mostro_client::cli::invocation::{
        parse_invocation, Invocation, OutputMode, OutputSelection,
    };
    use mostro_client::cli::{lookup, Cli, Commands, OrderCommands};
    use uuid::Uuid;

    const FIRST: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";
    const SECOND: &str = "00000000-0000-4000-8000-000000000002";

    fn parse(args: &[&str]) -> Result<Vec<Uuid>, clap::Error> {
        let argv = std::iter::once("mostro-cli").chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Order {
                command: OrderCommands::Info { orders },
            }) => Ok(orders),
            _ => panic!("not order info"),
        }
    }

    fn ids(texts: &[&str]) -> Vec<Uuid> {
        texts.iter().map(|text| text.parse().unwrap()).collect()
    }

    #[test]
    fn takes_one_or_more_order_ids_in_any_case_and_keeps_them_as_given() {
        assert_eq!(
            parse(&["order", "info", "--order", FIRST, "--order", SECOND]).unwrap(),
            ids(&[FIRST, SECOND])
        );
        assert_eq!(
            parse(&["order", "info", "--order", &FIRST.to_uppercase()]).unwrap(),
            ids(&[FIRST])
        );
        // Deduplication is the operation's job; the grammar has no cap.
        let many: Vec<String> = (0..300).map(|n| Uuid::from_u128(n).to_string()).collect();
        let mut args = vec!["order", "info"];
        for id in &many {
            args.extend(["--order", id]);
        }
        assert_eq!(parse(&args).unwrap().len(), 300);
    }

    #[test]
    fn an_order_is_required_and_the_old_spelling_is_gone() {
        for args in [
            &["order", "info"][..],
            &["order", "info", "--order"],
            &["order", "info", "--order", "not-a-uuid"],
            &["order", "info", FIRST],
            &["order", "info", "-o", FIRST],
            &["order", "info", "--orderids", FIRST],
            &["ordersinfo", "-o", FIRST],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn is_registered_as_the_request_command_order_info() {
        let entry = lookup(&["order".to_string(), "info".to_string()]).expect("registered");
        assert_eq!(entry.canonical, "order.info");
        assert!(lookup(&["ordersinfo".to_string()]).is_none());
    }

    #[test]
    fn argument_errors_with_json_intent_keep_the_resolved_command() {
        use std::ffi::OsString;
        let run = |suffix: &[&str]| {
            let argv = std::iter::once("mostro-cli").chain(suffix.iter().copied());
            parse_invocation(
                <Cli as clap::CommandFactory>::command(),
                argv.map(OsString::from),
            )
        };
        for suffix in [
            &["--output", "json", "order", "info"][..],
            &["order", "info", "--output=json", "--order", "bad"],
            &["order", "--output", "json", "info", "--bogus"],
        ] {
            let Invocation::Rejected(failure) = run(suffix) else {
                panic!("{suffix:?} should be rejected");
            };
            assert_eq!(
                failure.output,
                OutputSelection::Mode(OutputMode::Json),
                "{suffix:?}"
            );
            assert_eq!(
                failure.command,
                Some(vec!["order".to_string(), "info".to_string()]),
                "{suffix:?}"
            );
        }
        // A valid invocation parses and is not rejected for JSON.
        assert!(matches!(
            run(&["--output", "json", "order", "info", "--order", FIRST]),
            Invocation::Parsed {
                output: OutputMode::Json,
                ..
            }
        ));
    }
}

/// What the legacy `neworder` accepts today (spec 003, slice 1). Removed with
/// the command in slice 4; the new grammar gets its own tests then. Long flags
/// are squashed (`--fiatcode`, `--paymentmethod`, `--expirationdays`).
mod neworder_characterization {
    use clap::Parser;
    use mostro_client::cli::new_order::{confirm_new_order, validate_market_fiat, FiatNames};
    use mostro_client::cli::{Cli, Commands};

    type Fields = (
        String,
        i64,
        String,
        (i64, Option<i64>),
        String,
        i64,
        Option<String>,
        i64,
    );

    fn parse(args: &[&str]) -> Result<Fields, clap::Error> {
        let argv = std::iter::once("mostro-cli").chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::NewOrder {
                kind,
                amount,
                fiat_code,
                fiat_amount,
                payment_method,
                premium,
                invoice,
                expiration_days,
            }) => Ok((
                kind,
                amount,
                fiat_code,
                fiat_amount,
                payment_method,
                premium,
                invoice,
                expiration_days,
            )),
            _ => panic!("not neworder"),
        }
    }

    const BASE: [&str; 9] = [
        "neworder", "-k", "sell", "-c", "USD", "-f", "100", "-m", "SEPA",
    ];

    fn with(tail: &[&'static str]) -> Vec<&'static str> {
        BASE.iter().copied().chain(tail.iter().copied()).collect()
    }

    #[test]
    fn required_flags_and_defaults() {
        let (kind, amount, code, fiat, method, premium, invoice, days) = parse(&BASE).unwrap();
        assert_eq!((kind.as_str(), amount, code.as_str()), ("sell", 0, "USD"));
        assert_eq!((fiat, method.as_str()), ((100, None), "SEPA"));
        assert_eq!((premium, invoice, days), (0, None, 0));
        for missing in [3, 5, 7, 8] {
            let mut args = BASE.to_vec();
            args.drain(missing - 1..=missing);
            assert!(
                parse(&args).is_err(),
                "without {:?}",
                &BASE[missing - 1..=missing]
            );
        }
    }

    #[test]
    fn every_short_and_long_spelling_and_the_kind_text_is_not_checked_by_the_grammar() {
        let long = [
            "neworder",
            "--kind",
            "BUY",
            "--amount",
            "5000",
            "--fiatcode",
            "eur",
            "--fiatamount",
            "10-20",
            "--paymentmethod",
            "SEPA",
            "--premium",
            "-3",
            "--invoice",
            "lnbc1",
            "--expirationdays",
            "7",
        ];
        let (kind, amount, code, fiat, _, premium, invoice, days) = parse(&long).unwrap();
        assert_eq!((kind.as_str(), amount, code.as_str()), ("BUY", 5000, "eur"));
        assert_eq!(
            (fiat, premium, invoice.as_deref(), days),
            ((10, Some(20)), -3, Some("lnbc1"), 7)
        );
        let short = with(&["-a", "1", "-p", "-5", "-i", "x", "-e", "2"]);
        let parsed = parse(&short).unwrap();
        assert_eq!((parsed.1, parsed.5, parsed.7), (1, -5, 2));
        // A nonsense kind parses; `execute_new_order` rejects it later.
        assert_eq!(parse(&with(&[])).unwrap().0, "sell");
        let odd = [
            "neworder", "-k", "sideways", "-c", "USD", "-f", "1", "-m", "x",
        ];
        assert_eq!(parse(&odd).unwrap().0, "sideways");
    }

    #[test]
    fn the_fiat_range_syntax_and_its_failures() {
        let fiat = |text: &'static str| {
            let args = ["neworder", "-k", "sell", "-c", "USD", "-f", text, "-m", "x"];
            parse(&args).map(|f| f.3)
        };
        assert_eq!(fiat("100").unwrap(), (100, None));
        assert_eq!(fiat("100-200").unwrap(), (100, Some(200)));
        for bad in [
            "200-100", "100-100", "1-2-3", "100-", "-100", "abc", "1.5", "",
        ] {
            assert!(fiat(bad).is_err(), "{bad:?} must not parse");
        }
        // Zero and negative single amounts parse; the node refuses them.
        assert_eq!(fiat("0").unwrap(), (0, None));
    }

    #[test]
    fn the_confirmation_prompt_accepts_blank_and_y_and_never_eof() {
        let ask = |text: &str| confirm_new_order(text.as_bytes()).map_err(|e| e.to_string());
        // Only the end of the line is trimmed, so whitespace alone is blank.
        for yes in ["\n", "y\n", "Y\n", "  \n", "y  \n"] {
            assert!(ask(yes).is_ok(), "{yes:?}");
        }
        assert!(ask(" y\n").is_err(), "leading space is not trimmed");
        assert_eq!(ask("n\n").unwrap_err(), "Order cancelled");
        assert!(ask("maybe\n")
            .unwrap_err()
            .contains("Invalid order confirmation"));
        assert!(ask("").unwrap_err().contains("stdin reached EOF"));
    }

    #[test]
    fn a_market_priced_currency_must_be_in_the_yadio_list() {
        let names: FiatNames = [("USD".to_string(), "Dollar".to_string())].into();
        assert!(validate_market_fiat("USD", &names).is_ok());
        assert!(validate_market_fiat("XXX", &names)
            .unwrap_err()
            .to_string()
            .contains("not present in the fiat market"));
    }
}
