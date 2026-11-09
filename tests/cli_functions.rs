use mostro_core::prelude::*;
use uuid::Uuid;

// Rating range checks
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
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

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
        let order_new = [
            "order",
            "new",
            "--side",
            "sell",
            "--currency",
            "USD",
            "--fiat-amount",
            "100",
            "--payment-method",
            "SEPA",
        ];
        let with = |tail: &[&'static str]| -> Vec<&'static str> {
            order_new
                .iter()
                .copied()
                .chain(tail.iter().copied())
                .collect()
        };
        assert_eq!(
            parsed(&with(&["--premium", "-5", "--output", "json"])),
            (
                OutputMode::Json,
                vec!["order".to_string(), "new".to_string()]
            )
        );
        check(&[
            // `--premium` accepts hyphen values, so it swallows `--output`.
            (&with(&["--premium", "--output", "json"]), Expect::Human),
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
            &["admcancel", "--orderid", ORDER],
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
                &["--output", "json", "admcancel"],
                Expect::Json(Some(&["admcancel"])),
            ),
            (
                &["--output", "json", "admcancel", "--orderid", "not-a-uuid"],
                Expect::Json(Some(&["admcancel"])),
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
            (&["admcancel"], Expect::Human),
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
    use mostro_client::outcome::{DaemonAcceptance, ExecutionMeta, Persistence, Publication};
    use std::collections::BTreeSet;

    /// Canonical names approved on 2026-10-22, in subcommand order. T030
    /// replaced `listorders` with `order list`, keeping `order.list`.
    const APPROVED: [(&[&str], &str); 41] = [
        (&["order", "list"], "order.list"),
        (&["schema"], "schema"),
        (&["message", "list"], "message.list"),
        (&["getdmuser"], "chat.history"),
        (&["disputechat"], "dispute.chat"),
        (&["getadmindm"], "admin.message-list"),
        (&["senddm"], "message.send"),
        (&["dmtouser"], "chat.send"),
        (&["restore"], "account.restore"),
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
        (&["order", "new"], "order.new"),
        (&["order", "take-sell"], "order.take-sell"),
        (&["order", "take-buy"], "order.take-buy"),
        (&["order", "add-invoice"], "order.add-invoice"),
        (&["order", "fiat-sent"], "order.fiat-sent"),
        (&["order", "release"], "order.release"),
        (&["order", "cancel"], "order.cancel"),
        (&["order", "sync"], "order.sync"),
        (&["bond", "add-invoice"], "bond.add-invoice"),
        (&["identity", "init"], "identity.init"),
        (&["identity", "show"], "identity.show"),
        (&["order", "mine"], "order.mine"),
        (&["order", "rate"], "order.rate"),
        (&["dispute", "open"], "dispute.open"),
        (&["config", "set"], "config.set"),
        (&["config", "show"], "config.show"),
        (&["order", "chat"], "order.chat"),
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
                "order.list" | "schema" | "message.list" | "order.sync" | "identity.init"
                | "identity.show" | "order.mine" | "config.set" | "config.show" => {
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
                "order.new" | "order.take-sell" | "order.take-buy" | "order.add-invoice"
                | "order.fiat-sent" | "order.release" | "order.cancel" | "bond.add-invoice"
                | "order.rate" | "dispute.open" => {
                    assert_eq!(entry.json, JsonSupport::Write);
                    assert_eq!(
                        entry.stages::<()>(),
                        ExecutionMeta::write(
                            Publication::NotAttempted,
                            DaemonAcceptance::Unknown,
                            Persistence::NotAttempted
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
            lookup(&path(&["order", "new"])).map(|e| e.canonical),
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
        assert_eq!(
            machine,
            [
                "order.list",
                "schema",
                "message.list",
                "order.info",
                "order.new",
                "order.take-sell",
                "order.take-buy",
                "order.add-invoice",
                "order.fiat-sent",
                "order.release",
                "order.cancel",
                "order.sync",
                "bond.add-invoice",
                "identity.init",
                "identity.show",
                "order.mine",
                "order.rate",
                "dispute.open",
                "config.set",
                "config.show"
            ]
        );
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

/// The grammar of `order new` (spec 003). The legacy `neworder` and its short
/// flags, squashed long flags and `100-200` range syntax are gone.
mod order_new_grammar {
    use clap::Parser;
    use mostro_client::cli::{lookup, Cli, Commands, JsonSupport, NewArgs, OrderCommands};

    fn parse(args: &[&str]) -> Result<NewArgs, clap::Error> {
        let argv = ["mostro-cli", "order", "new"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Order {
                command: OrderCommands::New(args),
            }) => Ok(args),
            _ => panic!("not order new"),
        }
    }

    const BASE: [&str; 8] = [
        "--side",
        "sell",
        "--currency",
        "USD",
        "--fiat-amount",
        "100",
        "--payment-method",
        "SEPA",
    ];

    fn with(tail: &[&'static str]) -> Vec<&'static str> {
        BASE.iter().copied().chain(tail.iter().copied()).collect()
    }

    #[test]
    fn required_options_and_defaults() {
        let args = parse(&BASE).unwrap();
        assert_eq!(
            (args.side.as_str(), args.currency.as_str(), args.fiat_amount),
            ("sell", "USD", 100)
        );
        assert_eq!(args.payment_method, "SEPA");
        assert_eq!(args.fiat_amount_max, None);
        assert_eq!((args.sats, args.premium, args.expiration_days), (0, 0, 0));
        assert_eq!((args.invoice, args.yes), (None, false));
        for index in [0, 2, 4, 6] {
            let mut missing = BASE.to_vec();
            missing.drain(index..index + 2);
            assert!(parse(&missing).is_err(), "without {}", BASE[index]);
        }
    }

    #[test]
    fn every_option_and_a_negative_premium() {
        let all = with(&[
            "--fiat-amount-max",
            "300",
            "--sats",
            "5000",
            "--premium",
            "-3",
            "--invoice",
            "lnbc1",
            "--expiration-days",
            "7",
            "--yes",
        ]);
        let args = parse(&all).unwrap();
        assert_eq!(args.fiat_amount_max, Some(300));
        assert_eq!(
            (args.sats, args.premium, args.expiration_days),
            (5000, -3, 7)
        );
        assert_eq!(args.invoice.as_deref(), Some("lnbc1"));
        assert!(args.yes);
        assert_eq!(
            parse(&[
                "--side",
                "BUY",
                "--currency",
                "eur",
                "--fiat-amount",
                "1",
                "--payment-method",
                "x"
            ])
            .unwrap()
            .side,
            "BUY"
        );
    }

    #[test]
    fn the_side_is_checked_and_text_amounts_are_rejected() {
        let swap = |option: &'static str, value: &'static str| {
            let mut args = BASE.to_vec();
            let at = args.iter().position(|a| *a == option).unwrap();
            args[at + 1] = value;
            parse(&args)
        };
        assert!(swap("--side", "sideways").is_err());
        for bad in ["1.5", "abc", "100-200", "", "-5x"] {
            assert!(swap("--fiat-amount", bad).is_err(), "{bad:?}");
        }
        assert!(parse(&with(&["--sats", "x"])).is_err());
        assert!(parse(&with(&["--premium", "1.5"])).is_err());
        assert!(parse(&with(&["--fiat-amount-max", "100-200"])).is_err());
    }

    #[test]
    fn the_legacy_spellings_are_gone() {
        let try_args = |args: &[&str]| Cli::try_parse_from(args.iter().copied());
        assert!(try_args(&[
            "mostro-cli",
            "neworder",
            "-k",
            "sell",
            "-c",
            "USD",
            "-f",
            "100",
            "-m",
            "SEPA"
        ])
        .is_err());
        for legacy in [
            ["-k", "sell"],
            ["--kind", "sell"],
            ["--fiatcode", "USD"],
            ["--fiatamount", "100"],
            ["--paymentmethod", "SEPA"],
            ["--expirationdays", "3"],
            ["-a", "5"],
            ["-p", "5"],
        ] {
            assert!(parse(&with(&legacy)).is_err(), "{legacy:?}");
        }
    }

    #[test]
    fn the_registry_describes_a_json_capable_write() {
        let entry = lookup(&["order".to_string(), "new".to_string()]).unwrap();
        assert_eq!(entry.canonical, "order.new");
        assert_eq!(entry.json, JsonSupport::Write);
        assert!(lookup(&["neworder".to_string()]).is_none());
    }
}

/// The grammar of `order take-sell` and `order take-buy` (spec 004). The legacy
/// `takesell` and `takebuy`, their short flags and the squashed `--orderid` are
/// gone.
mod order_take_grammar {
    use clap::Parser;
    use mostro_client::cli::{
        lookup, Cli, Commands, JsonSupport, OrderCommands, TakeBuyArgs, TakeSellArgs,
    };

    const ID: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";

    fn sell(args: &[&str]) -> Result<TakeSellArgs, clap::Error> {
        let argv = ["mostro-cli", "order", "take-sell"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Order {
                command: OrderCommands::TakeSell(args),
            }) => Ok(args),
            _ => panic!("not take-sell"),
        }
    }

    fn buy(args: &[&str]) -> Result<TakeBuyArgs, clap::Error> {
        let argv = ["mostro-cli", "order", "take-buy"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Order {
                command: OrderCommands::TakeBuy(args),
            }) => Ok(args),
            _ => panic!("not take-buy"),
        }
    }

    #[test]
    fn take_sell_takes_an_order_an_optional_invoice_and_amount_and_yes() {
        let args = sell(&["--order", ID]).unwrap();
        assert_eq!(args.order.to_string(), ID);
        assert_eq!((args.invoice, args.amount, args.yes), (None, None, false));
        let args = sell(&[
            "--order",
            &ID.to_uppercase(),
            "--invoice",
            "lnbc1",
            "--amount",
            "150",
            "--yes",
        ])
        .unwrap();
        assert_eq!(args.order.to_string(), ID);
        assert_eq!(args.invoice.as_deref(), Some("lnbc1"));
        assert_eq!((args.amount, args.yes), (Some(150), true));
    }

    #[test]
    fn take_buy_has_no_invoice_option() {
        let args = buy(&["--order", ID, "--amount", "150", "--yes"]).unwrap();
        assert_eq!((args.amount, args.yes), (Some(150), true));
        assert!(buy(&["--order", ID, "--invoice", "lnbc1"]).is_err());
    }

    #[test]
    fn the_order_is_required_and_text_amounts_are_rejected() {
        for bad in [
            sell(&[]).err(),
            sell(&["--order", "not-a-uuid"]).err(),
            sell(&["--order", ID, "--amount", "1.5"]).err(),
            sell(&["--order", ID, "--amount", "abc"]).err(),
            buy(&[]).err(),
            buy(&["--order", ID, "--amount", "x"]).err(),
        ] {
            assert!(bad.is_some());
        }
    }

    #[test]
    fn the_legacy_spellings_are_gone() {
        let try_args = |args: &[&str]| Cli::try_parse_from(args.iter().copied());
        assert!(try_args(&["mostro-cli", "takesell", "--orderid", ID]).is_err());
        assert!(try_args(&["mostro-cli", "takebuy", "-o", ID]).is_err());
        for legacy in [
            ["-o", ID],
            ["--order-id", ID],
            ["--orderid", ID],
            ["-i", "lnbc1"],
            ["-a", "5"],
        ] {
            let mut all = vec!["--order", ID];
            all.extend(legacy);
            assert!(sell(&all).is_err(), "{legacy:?}");
        }
    }

    #[test]
    fn both_are_registered_json_capable_writes() {
        for (path, name) in [
            ("take-sell", "order.take-sell"),
            ("take-buy", "order.take-buy"),
        ] {
            let entry = lookup(&["order".to_string(), path.to_string()]).unwrap();
            assert_eq!(entry.canonical, name);
            assert_eq!(entry.json, JsonSupport::Write);
        }
        assert!(lookup(&["takesell".to_string()]).is_none());
        assert!(lookup(&["takebuy".to_string()]).is_none());
    }
}

/// The grammar of `order add-invoice` and `bond add-invoice` (spec 005, slice 4).
mod invoice_grammar {
    use clap::Parser;
    use mostro_client::cli::{
        lookup, AddBondInvoiceArgs, AddInvoiceArgs, BondCommands, Cli, Commands, JsonSupport,
        OrderCommands,
    };

    const ID: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";

    fn order(args: &[&str]) -> Result<AddInvoiceArgs, clap::Error> {
        let argv = ["mostro-cli", "order", "add-invoice"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Order {
                command: OrderCommands::AddInvoice(args),
            }) => Ok(args),
            _ => panic!("not order add-invoice"),
        }
    }

    fn bond(args: &[&str]) -> Result<AddBondInvoiceArgs, clap::Error> {
        let argv = ["mostro-cli", "bond", "add-invoice"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Bond {
                command: BondCommands::AddInvoice(args),
            }) => Ok(args),
            _ => panic!("not bond add-invoice"),
        }
    }

    #[test]
    fn both_take_an_order_an_invoice_and_yes() {
        let args = order(&["--order", ID, "--invoice", "lnbc1x"]).unwrap();
        assert_eq!(args.order.to_string(), ID);
        assert_eq!((args.invoice.as_str(), args.yes), ("lnbc1x", false));
        let args = order(&["--order", &ID.to_uppercase(), "--invoice", "a@b.c", "--yes"]).unwrap();
        assert_eq!(args.order.to_string(), ID, "a UUID in any case");
        assert!(args.yes);

        let args = bond(&["--order", ID, "--invoice", "lnbc1x", "--yes"]).unwrap();
        assert_eq!(
            (args.order.to_string(), args.invoice.as_str()),
            (ID.to_string(), "lnbc1x")
        );
        assert!(args.yes);
    }

    #[test]
    fn the_order_and_the_invoice_are_required_and_the_old_spellings_are_gone() {
        for parse in [
            |a: &[&str]| order(a).map(|_| ()),
            |a: &[&str]| bond(a).map(|_| ()),
        ] {
            assert!(parse(&[]).is_err());
            assert!(parse(&["--order", ID]).is_err());
            assert!(parse(&["--invoice", "lnbc1x"]).is_err());
            assert!(parse(&["--order", "not-a-uuid", "--invoice", "lnbc1x"]).is_err());
            for old in [
                &["-o", ID, "-i", "lnbc1x"][..],
                &["--orderid", ID, "--invoice", "lnbc1x"],
                &["--order-id", ID, "--invoice", "lnbc1x"],
            ] {
                assert!(parse(old).is_err(), "{old:?}");
            }
        }
    }

    #[test]
    fn the_legacy_commands_are_gone_with_no_alias() {
        for name in ["addinvoice", "addbondinvoice"] {
            assert!(Cli::try_parse_from(["mostro-cli", name, "-o", ID, "-i", "x"]).is_err());
            assert!(lookup(&[name.to_string()]).is_none(), "{name}");
        }
    }

    #[test]
    fn both_are_registered_write_commands_with_the_approved_names() {
        for (path, canonical) in [
            (["order", "add-invoice"], "order.add-invoice"),
            (["bond", "add-invoice"], "bond.add-invoice"),
        ] {
            let path = path.map(str::to_string);
            let entry = lookup(&path).expect(canonical);
            assert_eq!(entry.canonical, canonical);
            assert_eq!(entry.json, JsonSupport::Write, "{canonical}");
        }
    }
}

/// The grammar of `message list` (spec 006, slice 4): `getdm` and its options
/// are gone with no alias.
mod message_list_grammar {
    use clap::Parser;
    use mostro_client::cli::{lookup, Cli, Commands, JsonSupport, MessageCommands};
    use uuid::Uuid;

    fn parse(args: &[&str]) -> Result<(Option<i64>, Option<Uuid>), clap::Error> {
        let argv = ["mostro-cli", "message", "list"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Message {
                command: MessageCommands::List(args),
            }) => Ok((args.since, args.order)),
            _ => panic!("not message list"),
        }
    }

    #[test]
    fn both_options_are_optional_and_unset_means_the_default() {
        assert_eq!(parse(&[]).unwrap(), (None, None));
    }

    #[test]
    fn the_window_and_the_order_are_long_options_only() {
        let id = "00000000-0000-0000-0000-0000000000AB";
        assert_eq!(
            parse(&["--since", "90", "--order", id]).unwrap(),
            (Some(90), Some(id.parse().unwrap()))
        );
        for short in [&["-s", "90"][..], &["-o", id], &["-f"]] {
            assert!(parse(short).is_err(), "{short:?}");
        }
    }

    #[test]
    fn range_is_the_operation_s_to_judge_but_non_numbers_and_bad_ids_fail_here() {
        assert_eq!(parse(&["--since", "0"]).unwrap().0, Some(0));
        assert_eq!(parse(&["--since=-5"]).unwrap().0, Some(-5));
        assert!(parse(&["--since", "soon"]).is_err());
        assert!(parse(&["--since", "1.5"]).is_err());
        assert!(parse(&["--order", "not-a-uuid"]).is_err());
    }

    #[test]
    fn peer_chat_and_the_old_names_are_removed_with_no_alias() {
        assert!(parse(&["--from-user"]).is_err());
        assert!(parse(&["--fromuser"]).is_err());
        for old in [&["getdm"][..], &["getdm", "--since", "5"]] {
            let argv = ["mostro-cli"].into_iter().chain(old.iter().copied());
            assert!(Cli::try_parse_from(argv).is_err(), "{old:?}");
        }
    }

    #[test]
    fn it_is_a_registered_read_with_the_canonical_name() {
        let path = ["message".to_string(), "list".to_string()];
        let entry = lookup(&path).expect("registered");
        assert_eq!(entry.canonical, "message.list");
        assert_eq!(entry.json, JsonSupport::Read);
        assert!(lookup(&["getdm".to_string()]).is_none());
    }
}

/// The grammar of `order fiat-sent`, `order release` and `order cancel` (spec
/// 007, slice 4).
mod trade_step_grammar {
    use clap::Parser;
    use mostro_client::cli::{lookup, Cli, Commands, JsonSupport, OrderCommands, StepArgs};

    const ID: &str = "61bd0415-a9bc-4c9c-a1fc-bb2c305cf3b0";
    const STEPS: [(&str, &str); 3] = [
        ("fiat-sent", "order.fiat-sent"),
        ("release", "order.release"),
        ("cancel", "order.cancel"),
    ];

    fn parse(step: &str, args: &[&str]) -> Result<StepArgs, clap::Error> {
        let argv = ["mostro-cli", "order", step]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Order {
                command:
                    OrderCommands::FiatSent(args)
                    | OrderCommands::Release(args)
                    | OrderCommands::Cancel(args),
            }) => Ok(args),
            _ => panic!("not a trade step"),
        }
    }

    #[test]
    fn each_takes_an_order_and_an_optional_yes() {
        for (step, _) in STEPS {
            let args = parse(step, &["--order", ID]).unwrap();
            assert_eq!((args.order.to_string(), args.yes), (ID.to_string(), false));
            let args = parse(step, &["--order", &ID.to_uppercase(), "--yes"]).unwrap();
            assert_eq!(args.order.to_string(), ID, "{step}: a UUID in any case");
            assert!(args.yes);
        }
    }

    #[test]
    fn the_order_is_required_and_the_old_spellings_are_gone() {
        for (step, _) in STEPS {
            assert!(parse(step, &[]).is_err(), "{step}");
            assert!(parse(step, &["--yes"]).is_err(), "{step}");
            assert!(parse(step, &["--order", "not-a-uuid"]).is_err(), "{step}");
            for old in [
                &["-o", ID][..],
                &["--orderid", ID],
                &["--order-id", ID],
                &["--order", ID, "--invoice", "x"],
            ] {
                assert!(parse(step, old).is_err(), "{step} {old:?}");
            }
        }
    }

    #[test]
    fn the_legacy_commands_are_gone_with_no_alias() {
        for name in ["fiatsent", "release", "cancel"] {
            assert!(Cli::try_parse_from(["mostro-cli", name, "-o", ID]).is_err());
            assert!(lookup(&[name.to_string()]).is_none(), "{name}");
        }
    }

    #[test]
    fn all_three_are_registered_write_commands_with_the_approved_names() {
        for (step, canonical) in STEPS {
            let entry = lookup(&["order".to_string(), step.to_string()]).expect(canonical);
            assert_eq!(entry.canonical, canonical);
            assert_eq!(entry.json, JsonSupport::Write, "{canonical}");
        }
    }
}

/// The grammar of `order sync` (spec 008, slice 4).
mod order_sync_grammar {
    use clap::Parser;
    use mostro_client::cli::{lookup, Cli, Commands, JsonSupport, OrderCommands, SyncArgs};

    fn parse(args: &[&str]) -> Result<SyncArgs, clap::Error> {
        let argv = ["mostro-cli", "order", "sync"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Order {
                command: OrderCommands::Sync(args),
            }) => Ok(args),
            _ => panic!("not order sync"),
        }
    }

    #[test]
    fn it_takes_only_an_optional_window() {
        assert_eq!(parse(&[]).unwrap().since, None);
        assert_eq!(parse(&["--since", "60"]).unwrap().since, Some(60));
        for bad in [
            &["--since"][..],
            &["--since", "x"],
            &["--yes"],
            &["--order", "x"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn it_is_registered_with_json_and_the_approved_name() {
        let entry = lookup(&["order".to_string(), "sync".to_string()]).expect("registered");
        assert_eq!(entry.canonical, "order.sync");
        assert_eq!(entry.json, JsonSupport::Read);
    }
}

/// The grammar of `identity init` and `identity show` (spec 009).
mod identity_grammar {
    use clap::Parser;
    use mostro_client::cli::{lookup, Cli, Commands, IdentityCommands, JsonSupport};

    fn parse(args: &[&str]) -> Result<IdentityCommands, clap::Error> {
        let argv = ["mostro-cli", "identity"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Identity { command }) => Ok(command),
            _ => panic!("not identity"),
        }
    }

    #[test]
    fn both_commands_take_no_options() {
        assert!(matches!(parse(&["init"]), Ok(IdentityCommands::Init)));
        assert!(matches!(parse(&["show"]), Ok(IdentityCommands::Show)));
        for bad in [
            &["init", "--yes"][..],
            &["show", "--reveal"],
            &["init", "extra"],
            &["new"],
            &[],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn both_are_registered_reads_with_the_approved_names() {
        for (verb, canonical) in [("init", "identity.init"), ("show", "identity.show")] {
            let entry = lookup(&["identity".to_string(), verb.to_string()]).expect(canonical);
            assert_eq!(entry.canonical, canonical);
            // Publishes nothing, like `order sync`: rejected invocations report
            // read stages.
            assert_eq!(entry.json, JsonSupport::Read, "{canonical}");
        }
    }
}

/// The grammar of `config set` (spec 018).
mod config_grammar {
    use clap::{CommandFactory, Parser};
    use mostro_client::cli::invocation::{parse_invocation, Invocation};
    use mostro_client::cli::{Cli, Commands, ConfigCommands};

    fn parse(args: &[&str]) -> Result<ConfigCommands, clap::Error> {
        let argv = ["mostro-cli", "config"]
            .into_iter()
            .chain(args.iter().copied());
        match Cli::try_parse_from(argv)?.command {
            Some(Commands::Config { command }) => Ok(command),
            _ => panic!("not config"),
        }
    }

    #[test]
    fn set_takes_a_known_key_and_one_value() {
        for key in ["mostro-pubkey", "relays", "pow", "transport"] {
            let Ok(ConfigCommands::Set(args)) = parse(&["set", key, "x"]) else {
                panic!("{key}");
            };
            assert_eq!((args.key.as_str(), args.value.as_str()), (key, "x"));
        }
        // After `--` a value may start with `-`; the value check refuses it.
        let Ok(ConfigCommands::Set(args)) = parse(&["set", "pow", "--", "-1"]) else {
            panic!("pow -- -1");
        };
        assert_eq!(args.value, "-1");
        for bad in [
            &["set", "admin-nsec", "x"][..],
            &["set", "MOSTRO_PUBKEY", "x"],
            &["set", "pow"],
            &["set", "pow", "1", "2"],
            &["set"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
            // In JSON these are invalid_arguments of config.set (exit 2).
            let argv = ["mostro-cli", "--output", "json", "config"]
                .into_iter()
                .chain(bad.iter().copied());
            let Invocation::Rejected(failure) = parse_invocation(Cli::command(), argv) else {
                panic!("{bad:?} was accepted");
            };
            assert_eq!(
                failure.command,
                Some(vec!["config".to_string(), "set".to_string()]),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn show_takes_no_argument() {
        assert!(matches!(parse(&["show"]), Ok(ConfigCommands::Show)));
        for bad in [&["show", "pow"][..], &["show", "--key", "pow"]] {
            assert!(parse(bad).is_err(), "{bad:?}");
            let argv = ["mostro-cli", "--output", "json", "config"]
                .into_iter()
                .chain(bad.iter().copied());
            let Invocation::Rejected(failure) = parse_invocation(Cli::command(), argv) else {
                panic!("{bad:?} was accepted");
            };
            let show = vec!["config".to_string(), "show".to_string()];
            assert_eq!(failure.command, Some(show), "{bad:?}");
        }
    }
}
