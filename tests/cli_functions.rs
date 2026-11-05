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
