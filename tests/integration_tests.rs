use std::collections::BTreeMap;
use std::fs::{read_dir, read_to_string};
use std::path::Path;

use chrono::DateTime;
use mostro_client::cli::{offline_discovery, Context};
use mostro_client::outcome::dispute::{DisputeOpenData, COMMAND as DISPUTE_OPEN};
use mostro_client::outcome::identity::{
    IdentityInitData, IdentityShowData, INIT_COMMAND as IDENTITY_INIT,
    SHOW_COMMAND as IDENTITY_SHOW,
};
use mostro_client::outcome::info::{OrderInfoData, OrderInfoInput};
use mostro_client::outcome::invoice::{
    Answered, BondInvoiceData, InvoiceInput, InvoiceKind, InvoiceRequest, OrderInvoiceData,
};
use mostro_client::outcome::messages::{MessageListData, MessageListInput, MessageRecord};
use mostro_client::outcome::mine::{MineOrder, OrderMineData, COMMAND as ORDER_MINE};
use mostro_client::outcome::new::{OrderNewData, OrderNewInput, OrderRequest};
use mostro_client::outcome::order::ListOrdersInput;
use mostro_client::outcome::rate::{RateData, RateInput, COMMAND as ORDER_RATE};
use mostro_client::outcome::take::{OrderTakeData, OrderTakeInput, TakeRequest, TakeSide, Taken};
use mostro_client::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, WarningCode,
    Warnings, INVALID_INVOICE,
};
use mostro_client::util::events::create_seven_days_filter_at;
use mostro_client::util::{create_filter, select_reply_event, ExpectedReply, ListKind};
use mostro_core::order::Status;
use mostro_core::prelude::{
    wrap_message_with, Action, CantDoReason, Message, MessageKind, Payload, PaymentFailedInfo,
    Transport, UserInfo, WrapOptions,
};
use nostr_sdk::prelude::*;
use rstest::rstest;
use serde_json::{json, to_value, Value};
use sqlx::SqlitePool;

#[path = "common/schema_check.rs"]
mod schema_check;

// Helper to create a test context for integration tests
async fn create_test_context() -> anyhow::Result<Context> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;

    // Generate test keys
    let identity_keys = Keys::generate();
    let trade_keys = Keys::generate();
    let context_keys = Keys::generate();

    // Create a test client
    let client = Client::builder()
        .authenticator(SignerAuthenticator::new(identity_keys.clone()))
        .build();

    // Mock mostro pubkey
    let mostro_pubkey = PublicKey::from_hex(&format!("02{}", "1".repeat(62)))?;

    Ok(Context {
        client,
        identity_keys,
        trade_keys,
        trade_index: 0,
        pool,
        context_keys: Some(context_keys),
        mostro_pubkey,
    })
}

#[tokio::test]
async fn test_context_creation() {
    let result = create_test_context().await;
    assert!(result.is_ok());

    let ctx = result.unwrap();
    assert_eq!(ctx.trade_index, 0);
}

#[tokio::test]
async fn test_context_fields_are_valid() {
    let ctx = create_test_context().await.unwrap();

    // Verify all required fields are present and valid
    assert!(!ctx.identity_keys.public_key().to_hex().is_empty());
    assert!(!ctx.identity_keys.public_key().to_hex().is_empty());
    assert!(!ctx.trade_keys.public_key().to_hex().is_empty());
    assert!(!ctx
        .context_keys
        .as_ref()
        .unwrap()
        .public_key()
        .to_hex()
        .is_empty());
    assert!(!ctx.mostro_pubkey.to_hex().is_empty());
    assert!(!ctx.pool.is_closed());
}

#[tokio::test]
async fn test_filter_creation_integration() {
    let ctx = create_test_context().await.unwrap();

    let before = Timestamp::now().as_secs();
    let filter =
        create_filter(ListKind::Orders, ctx.mostro_pubkey, None, ctx.mostro_pubkey).unwrap();
    let after = Timestamp::now().as_secs();
    let since = filter.since.unwrap().as_secs();

    assert!((before - 604_800..=after - 604_800).contains(&since));
    assert_eq!(
        to_value(filter).unwrap(),
        json!({
            "authors": [ctx.mostro_pubkey.to_hex()],
            "kinds": [38383],
            "#z": ["order"],
            "since": since,
            "limit": 50
        })
    );
}

#[test]
fn order_query_uses_exact_seven_day_window_at_fixed_time() {
    let author = Keys::generate().public_key();
    let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let filter = create_seven_days_filter_at("order".to_string(), author, 38383, now).unwrap();

    assert_eq!(
        to_value(filter).unwrap(),
        json!({
            "authors": [author.to_hex()],
            "kinds": [38383],
            "#z": ["order"],
            "since": 1_699_395_200,
            "limit": 50
        })
    );
}

#[rstest]
#[case(None, Status::Pending)]
#[case(Some("PeNdInG"), Status::Pending)]
#[case(Some("ACTIVE"), Status::Active)]
fn listing_status_defaults_and_case(#[case] input: Option<&str>, #[case] expected: Status) {
    let filters = ListOrdersInput::parse(None, None, input).unwrap();
    assert_eq!(filters.status(), expected);
}

#[rstest]
#[case("")]
#[case(" pending")]
#[case("pending ")]
#[case("unknown")]
fn listing_status_rejects_invalid_or_padded_values(#[case] input: &str) {
    assert!(ListOrdersInput::parse(None, None, Some(input)).is_err());
}

// `create_filter` for the `DirectMessages*` kinds selects kind 14 and pins
// `author = mostro_pubkey` (kind 14 is shared with NIP-17 peer chat, so the
// author pin is what disambiguates the Mostro reply).
#[test]
fn direct_messages_filter_is_transport_aware() {
    let trade = Keys::generate().public_key();
    let mostro = Keys::generate().public_key();

    let filter = create_filter(ListKind::DirectMessagesUser, trade, None, mostro).unwrap();
    assert!(filter
        .kinds
        .as_ref()
        .unwrap()
        .contains(&nostr_sdk::prelude::Kind::PrivateDirectMessage));
    assert!(
        filter.authors.as_ref().unwrap().contains(&mostro),
        "DM filter must pin author = mostro_pubkey"
    );
}

// The reply-wait selection (`src/util/messaging.rs`) must skip an unrelated
// kind-14 that reaches the trade key before the correlated reply, instead of
// racing on the first kind-14 it sees. These build real encrypted kind-14
// events and exercise the same decision the wait loop runs per event.

// Build an encrypted kind-14 the way Mostro sends a reply: `author` signs it
// (identity == trade == author) and addresses it to our trade key, so the CLI
// decrypts it with its trade keys on the Mostro-protocol path.
async fn mostro_kind14(author: &Keys, our_trade: &PublicKey, message: &Message) -> Event {
    wrap_message_with(
        Transport::Nip44Direct,
        message,
        author,
        author,
        *our_trade,
        WrapOptions::default(),
    )
    .await
    .expect("wrap kind-14 reply")
}

fn user_info_reply(request_id: u64) -> Message {
    Message::Restore(MessageKind::new(
        None,
        Some(request_id),
        None,
        Action::UserInfo,
        Some(Payload::UserInfo(UserInfo::default())),
    ))
}

#[tokio::test]
async fn correlating_wait_skips_unrelated_kind14_before_matching_reply() {
    let mostro = Keys::generate();
    let trade = Keys::generate();
    let expected = ExpectedReply::new(42, vec![Action::UserInfo, Action::CantDo]);

    // A stale/racing Mostro push with a different request_id reaches the trade
    // key first; the correlating wait must skip it.
    let unrelated = mostro_kind14(&mostro, &trade.public_key(), &user_info_reply(7)).await;
    assert!(
        !select_reply_event(
            &unrelated,
            &trade,
            mostro.public_key(),
            None,
            Some(&expected)
        )
        .await,
        "an unrelated kind-14 (wrong request_id) must not win the wait"
    );

    // The genuine reply that echoes the request_id is accepted.
    let matching = mostro_kind14(&mostro, &trade.public_key(), &user_info_reply(42)).await;
    assert!(
        select_reply_event(
            &matching,
            &trade,
            mostro.public_key(),
            None,
            Some(&expected)
        )
        .await,
        "the reply echoing the request_id must be accepted"
    );

    // Without correlation (plain wait_for_dm) the same unrelated event would be
    // accepted — which is why id-bearing flows must use wait_for_dm_matching.
    assert!(
        select_reply_event(&unrelated, &trade, mostro.public_key(), None, None).await,
        "the non-correlating path accepts any Mostro kind-14"
    );
}

#[tokio::test]
async fn correlating_wait_rejects_non_mostro_and_own_echo() {
    let mostro = Keys::generate();
    let stranger = Keys::generate();
    let trade = Keys::generate();
    let expected = ExpectedReply::new(42, vec![Action::UserInfo]);

    // NIP-17 peer chat from a stranger, tagged to the trade key with a matching
    // id, is still skipped because it is not authored by Mostro.
    let peer_chat = mostro_kind14(&stranger, &trade.public_key(), &user_info_reply(42)).await;
    assert!(
        !select_reply_event(
            &peer_chat,
            &trade,
            mostro.public_key(),
            None,
            Some(&expected)
        )
        .await,
        "a kind-14 not authored by Mostro must be skipped even with a matching id"
    );

    // The echo of our own request (admin flows author as Mostro) is skipped by
    // event id, but the identical event is accepted when it is not our echo.
    let reply = mostro_kind14(&mostro, &trade.public_key(), &user_info_reply(42)).await;
    assert!(
        !select_reply_event(
            &reply,
            &trade,
            mostro.public_key(),
            Some(reply.id),
            Some(&expected)
        )
        .await,
        "our own echoed request must be skipped by event id"
    );
    assert!(
        select_reply_event(&reply, &trade, mostro.public_key(), None, Some(&expected)).await,
        "the same reply is accepted when it is not our own echo"
    );
}

// --- Generated schema fixtures and proposed examples (T045) ---------------

mod schema_fixtures {
    use super::{
        json, offline_discovery, read_to_string, schema_check, to_value, DaemonAcceptance,
        Envelope, ErrorCode, ExecutionMeta, OrderInfoData, OrderInfoInput, OrderNewData,
        OrderNewInput, OrderRequest, OrderTakeData, OrderTakeInput, Persistence, PublicKey,
        Publication, TakeRequest, TakeSide, Taken, Value, Warnings,
    };
    use super::{
        Action, Answered, BondInvoiceData, CantDoReason, EventId, InvoiceInput, InvoiceKind,
        InvoiceRequest, Message, MessageListData, MessageListInput, MessageRecord,
        OrderInvoiceData, Payload, PaymentFailedInfo,
    };
    use super::{
        DisputeOpenData, MineOrder, OrderMineData, RateData, RateInput, WarningCode, DISPUTE_OPEN,
        INVALID_INVOICE, ORDER_MINE, ORDER_RATE,
    };
    use super::{IdentityInitData, IdentityShowData, IDENTITY_INIT, IDENTITY_SHOW};
    use mostro_core::order::{Kind as OrderKind, SmallOrder, Status as OrderStatus};
    use mostro_core::prelude::Peer;

    const DISCOVERY_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/discovery-v1.json"
    );
    const LISTING_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-list-v1.json"
    );
    const INFO_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-info-v1.json"
    );
    const NEW_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-new-v1.json"
    );
    const TAKE_SELL_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-take-sell-v1.json"
    );
    const TAKE_BUY_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-take-buy-v1.json"
    );
    const ADD_INVOICE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-add-invoice-v1.json"
    );
    const BOND_INVOICE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/bond-add-invoice-v1.json"
    );
    const FIAT_SENT_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-fiat-sent-v1.json"
    );
    const RELEASE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-release-v1.json"
    );
    const CANCEL_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-cancel-v1.json"
    );
    const SYNC_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-sync-v1.json"
    );
    const MESSAGE_LIST_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/message-list-v1.json"
    );
    const IDENTITY_INIT_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/identity-init-v1.json"
    );
    const IDENTITY_SHOW_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/identity-show-v1.json"
    );
    const ORDER_MINE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-mine-v1.json"
    );
    const ORDER_RATE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-rate-v1.json"
    );
    const DISPUTE_OPEN_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/dispute-open-v1.json"
    );
    const ORDER_CHAT_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-chat-v1.json"
    );
    const WATCH_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/schemas/order-watch-v1.json"
    );
    const EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/001-order-list-json/contracts/examples.json"
    );

    /// How to refresh the fixtures after an intended change. The tests only read
    /// them and never rewrite a fixture.
    const REGENERATE: &str =
        "regenerate with: cargo run -q -- schema > docs/schemas/discovery-v1.json; \
    for NAME in list info new take-sell take-buy add-invoice fiat-sent release cancel sync; do cargo run -q -- --output json schema | jq \".data.commands[] | \
    select(.name==\\\"order.$NAME\\\") | .output_schema\" > docs/schemas/order-$NAME-v1.json; done; \
    cargo run -q -- --output json schema | jq \".data.commands[] | select(.name==\\\"bond.add-invoice\\\") | \
    .output_schema\" > docs/schemas/bond-add-invoice-v1.json; \
    cargo run -q -- --output json schema | jq \".data.commands[] | select(.name==\\\"message.list\\\") | \
    .output_schema\" > docs/schemas/message-list-v1.json; \
    for NAME in init show; do cargo run -q -- --output json schema | jq \".data.commands[] | \
    select(.name==\\\"identity.$NAME\\\") | .output_schema\" > docs/schemas/identity-$NAME-v1.json; done; \
    for NAME in order.mine order.rate dispute.open order.chat order.watch; do cargo run -q -- --output json schema | jq \".data.commands[] | \
    select(.name==\\\"$NAME\\\") | .output_schema\" > docs/schemas/$(echo $NAME | tr . -)-v1.json; done";

    fn read_json(path: &str) -> Value {
        let text = read_to_string(path).unwrap_or_else(|error| panic!("{path}: {error}"));
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"))
    }

    fn generated_discovery() -> Value {
        to_value(offline_discovery()).unwrap()
    }

    fn generated_output_schema(name: &str) -> Value {
        generated_discovery()["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["name"] == name)
            .map(|command| command["output_schema"].clone())
            .unwrap_or_else(|| panic!("{name} is described"))
    }

    fn generated_listing_schema() -> Value {
        generated_output_schema("order.list")
    }

    #[test]
    fn discovery_fixture_matches_the_generated_document() {
        assert_eq!(
            generated_discovery(),
            read_json(DISCOVERY_FIXTURE),
            "stale docs/schemas/discovery-v1.json; {REGENERATE}"
        );
    }

    #[test]
    fn listing_schema_fixture_matches_the_generated_schema() {
        assert_eq!(
            generated_listing_schema(),
            read_json(LISTING_FIXTURE),
            "stale docs/schemas/order-list-v1.json; {REGENERATE}"
        );
    }

    #[test]
    fn info_schema_fixture_matches_the_generated_schema() {
        assert_eq!(
            generated_output_schema("order.info"),
            read_json(INFO_FIXTURE),
            "stale docs/schemas/order-info-v1.json; {REGENERATE}"
        );
    }

    #[test]
    fn new_schema_fixture_matches_the_generated_schema() {
        assert_eq!(
            generated_output_schema("order.new"),
            read_json(NEW_FIXTURE),
            "stale docs/schemas/order-new-v1.json; {REGENERATE}"
        );
    }

    #[test]
    fn take_schema_fixtures_match_the_generated_schemas() {
        for (name, fixture) in [
            ("order.take-sell", TAKE_SELL_FIXTURE),
            ("order.take-buy", TAKE_BUY_FIXTURE),
        ] {
            assert_eq!(
                generated_output_schema(name),
                read_json(fixture),
                "stale fixture of {name}; {REGENERATE}"
            );
        }
    }

    #[test]
    fn message_list_schema_fixture_matches_the_generated_schema() {
        assert_eq!(
            generated_output_schema("message.list"),
            read_json(MESSAGE_LIST_FIXTURE),
            "stale fixture of message.list; {REGENERATE}"
        );
    }

    #[test]
    fn invoice_schema_fixtures_match_the_generated_schemas() {
        for (name, fixture) in [
            ("order.add-invoice", ADD_INVOICE_FIXTURE),
            ("bond.add-invoice", BOND_INVOICE_FIXTURE),
        ] {
            assert_eq!(
                generated_output_schema(name),
                read_json(fixture),
                "stale fixture of {name}; {REGENERATE}"
            );
        }
    }

    #[test]
    fn sync_schema_fixture_matches_the_generated_schema() {
        assert_eq!(
            generated_output_schema("order.sync"),
            read_json(SYNC_FIXTURE),
            "stale fixture of order.sync; {REGENERATE}"
        );
    }

    #[test]
    fn trade_step_schema_fixtures_match_the_generated_schemas() {
        for (name, fixture) in [
            ("order.fiat-sent", FIAT_SENT_FIXTURE),
            ("order.release", RELEASE_FIXTURE),
            ("order.cancel", CANCEL_FIXTURE),
        ] {
            assert_eq!(
                generated_output_schema(name),
                read_json(fixture),
                "stale fixture of {name}; {REGENERATE}"
            );
        }
    }

    #[test]
    fn mine_rate_and_dispute_schema_fixtures_match_the_generated_schemas() {
        for (name, fixture) in [
            ("order.mine", ORDER_MINE_FIXTURE),
            ("order.rate", ORDER_RATE_FIXTURE),
            ("dispute.open", DISPUTE_OPEN_FIXTURE),
        ] {
            assert_eq!(
                generated_output_schema(name),
                read_json(fixture),
                "stale {fixture}; {REGENERATE}"
            );
        }
    }

    #[test]
    fn chat_schema_fixture_matches_the_generated_schema() {
        assert_eq!(
            generated_output_schema("order.chat"),
            read_json(ORDER_CHAT_FIXTURE),
            "stale {ORDER_CHAT_FIXTURE}; {REGENERATE}"
        );
    }

    #[test]
    fn watch_schema_fixture_matches_the_generated_schema() {
        assert_eq!(
            generated_output_schema("order.watch"),
            read_json(WATCH_FIXTURE),
            "stale {WATCH_FIXTURE}; {REGENERATE}"
        );
    }

    #[test]
    fn identity_schema_fixtures_match_the_generated_schemas() {
        for (name, fixture) in [
            ("identity.init", IDENTITY_INIT_FIXTURE),
            ("identity.show", IDENTITY_SHOW_FIXTURE),
        ] {
            assert_eq!(
                generated_output_schema(name),
                read_json(fixture),
                "stale {fixture}; {REGENERATE}"
            );
        }
    }

    #[test]
    fn discovery_fixture_is_raw_discovery_data_not_an_envelope() {
        let fixture = read_json(DISCOVERY_FIXTURE);
        let keys: Vec<&str> = fixture
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        // Parsed objects are key-sorted; the set is what the document promises.
        assert_eq!(
            keys,
            [
                "commands",
                "executable_version",
                "global_options",
                "schema_dialect",
                "schema_version"
            ]
        );
        assert_eq!(fixture["schema_version"], "1");
        assert_eq!(
            fixture["schema_dialect"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        let names: Vec<&str> = fixture["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|command| command["name"].as_str().unwrap())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let machine: Vec<&str> = fixture["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|command| command["output_modes"] == json!(["human", "json"]))
            .map(|command| command["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            machine,
            [
                "bond.add-invoice",
                "dispute.open",
                "identity.init",
                "identity.show",
                "message.list",
                "order.add-invoice",
                "order.cancel",
                "order.chat",
                "order.fiat-sent",
                "order.info",
                "order.list",
                "order.mine",
                "order.new",
                "order.rate",
                "order.release",
                "order.sync",
                "order.take-buy",
                "order.take-sell",
                "order.watch",
                "schema"
            ]
        );
    }

    /// A signed 64-bit integer in canonical decimal text, which a JSON Schema
    /// pattern cannot bound.
    fn is_canonical_i64(text: &str) -> bool {
        text.parse::<i64>()
            .is_ok_and(|number| number.to_string() == text)
    }

    const DECIMAL_FIELDS: [&str; 6] = [
        "sats_amount",
        "fiat_amount",
        "fiat_amount_min",
        "fiat_amount_max",
        "premium_percent",
        "created_at_unix",
    ];

    fn examples() -> Vec<(String, i32, Value)> {
        let document = read_json(EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    #[test]
    fn proposed_listing_examples_conform_to_the_checked_in_schema() {
        let schema = read_json(LISTING_FIXTURE);
        let mut checked = 0;
        for (name, exit, outcome) in examples() {
            if outcome["command"] != "order.list" {
                continue;
            }
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = outcome["error"]["code"]
                .as_str()
                .map_or(0, |code| match code {
                    "invalid_arguments" | "output_not_supported" => 2,
                    _ => 1,
                });
            assert_eq!(exit, expected, "{name}");
            for order in outcome["data"]["orders"].as_array().into_iter().flatten() {
                for field in DECIMAL_FIELDS {
                    if let Some(text) = order[field].as_str() {
                        assert!(is_canonical_i64(text), "{name}: {field} {text}");
                    }
                }
            }
            checked += 1;
        }
        assert_eq!(checked, 4);
    }

    #[test]
    fn the_legacy_rejection_example_matches_its_constructor_and_is_not_a_listing() {
        let (_, exit, outcome) = examples()
            .into_iter()
            .find(|(name, ..)| name == "unsupported_secret_command")
            .expect("the legacy rejection example exists");
        let built = Envelope::<(), ()>::error(
            Some("account.trade-key"),
            ErrorCode::OutputNotSupported,
            Warnings::new(),
            ExecutionMeta::not_executed(),
        );
        assert_eq!(to_value(&built).unwrap(), outcome);
        assert_eq!(built.exit_code(), exit);
        // Legacy commands have no generated schema; this outcome belongs to no
        // listing document.
        let schema = read_json(LISTING_FIXTURE);
        assert!(!schema_check::conforms(&schema, &schema, &outcome));
    }

    #[test]
    fn examples_stay_proposals_separate_from_generated_output() {
        // The examples are hand-authored design fixtures; only the files under
        // docs/schemas are generated and drift-checked.
        assert_eq!(read_json(EXAMPLES)["contract_status"], "proposed");
        for fixture in [DISCOVERY_FIXTURE, LISTING_FIXTURE, INFO_FIXTURE] {
            assert!(fixture.contains("/docs/schemas/"));
            assert!(read_json(fixture).get("cases").is_none());
        }
    }

    #[test]
    fn invalid_types_nulls_enums_and_amounts_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let schema = read_json(LISTING_FIXTURE);
        let base = examples()
            .into_iter()
            .find(|(name, ..)| name == "large_amount_success")
            .map(|(.., outcome)| outcome)
            .unwrap();
        assert!(schema_check::conforms(&schema, &schema, &base));
        let edits: Vec<(&str, Edit)> = vec![
            (
                "number for a decimal string",
                Box::new(|o| o["data"]["orders"][0]["fiat_amount"] = json!(15)),
            ),
            (
                "null id",
                Box::new(|o| o["data"]["orders"][0]["id"] = Value::Null),
            ),
            (
                "null side",
                Box::new(|o| o["data"]["orders"][0]["side"] = Value::Null),
            ),
            (
                "null created_at_unix",
                Box::new(|o| o["data"]["orders"][0]["created_at_unix"] = Value::Null),
            ),
            (
                "null snapshot on success",
                Box::new(|o| o["meta"]["snapshot"] = Value::Null),
            ),
            (
                "null data on success",
                Box::new(|o| o["data"] = Value::Null),
            ),
            (
                "unknown status",
                Box::new(|o| o["data"]["orders"][0]["status"] = json!("uncertain")),
            ),
            (
                "unknown side",
                Box::new(|o| o["data"]["orders"][0]["side"] = json!("both")),
            ),
            (
                "envelope status",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "unknown completion policy",
                Box::new(|o| o["meta"]["snapshot"]["completion_policy"] = json!("any")),
            ),
            (
                "negative zero",
                Box::new(|o| o["data"]["orders"][0]["sats_amount"] = json!("-0")),
            ),
            (
                "explicit plus",
                Box::new(|o| o["data"]["orders"][0]["premium_percent"] = json!("+5")),
            ),
            (
                "leading zero",
                Box::new(|o| o["data"]["orders"][0]["fiat_amount"] = json!("015")),
            ),
            (
                "fraction",
                Box::new(|o| o["data"]["orders"][0]["fiat_amount"] = json!("1.5")),
            ),
            (
                "exponent",
                Box::new(|o| o["data"]["orders"][0]["sats_amount"] = json!("1e3")),
            ),
            (
                "range endpoint as number",
                Box::new(|o| o["data"]["orders"][0]["fiat_amount_min"] = json!(10)),
            ),
            (
                "missing nullable key",
                Box::new(|o| {
                    o["data"]["orders"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("fiat_amount_max");
                }),
            ),
            (
                "complete history claimed",
                Box::new(|o| o["meta"]["snapshot"]["complete_history"] = json!(true)),
            ),
            (
                "error with data",
                Box::new(|o| o["error"] = json!({"code": "read_timeout", "message": "x"})),
            ),
        ];
        for (label, edit) in edits {
            let mut outcome = base.clone();
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label}"
            );
        }
    }

    #[test]
    fn the_signed_64_bit_range_is_a_semantic_rule_the_pattern_cannot_state() {
        let schema = read_json(LISTING_FIXTURE);
        let mut outcome = examples()
            .into_iter()
            .find(|(name, ..)| name == "large_amount_success")
            .map(|(.., outcome)| outcome)
            .unwrap();
        for good in ["9223372036854775807", "-9223372036854775808", "0", "-1"] {
            assert!(is_canonical_i64(good), "{good}");
            outcome["data"]["orders"][0]["sats_amount"] = json!(good);
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{good}");
        }
        // Over the range: the schema alone accepts the text, the semantic check
        // does not, and the length bound rejects only values longer than 20 characters.
        for over in ["9223372036854775808", "-9223372036854775809"] {
            assert!(!is_canonical_i64(over), "{over}");
            outcome["data"]["orders"][0]["sats_amount"] = json!(over);
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{over}");
        }
        outcome["data"]["orders"][0]["sats_amount"] = json!("922337203685477580700");
        assert!(!schema_check::conforms(&schema, &schema, &outcome));
        for bad in ["", "-0", "+1", "01", "1.0", "0x1", " 1"] {
            assert!(!is_canonical_i64(bad), "{bad:?}");
        }
    }

    // --- The proposed outcomes of `order info` (spec 002) --------------------

    const INFO_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/002-order-info-json/contracts/examples.json"
    );
    const NODE: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn info_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(INFO_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn info_example(name: &str) -> (i32, Value) {
        info_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    fn id(n: u128) -> uuid::Uuid {
        uuid::Uuid::from_u128(n)
    }

    fn small_order(n: u128, edit: impl FnOnce(&mut SmallOrder)) -> SmallOrder {
        let key = Some(NODE.to_string());
        let mut order = if n == 1 {
            SmallOrder::new(
                Some(id(1)),
                Some(OrderKind::Sell),
                Some(OrderStatus::Active),
                15_000,
                "USD".to_string(),
                None,
                None,
                15,
                "bank transfer".to_string(),
                1,
                None,
                key,
                None,
                Some(1_792_108_800),
                Some(1_792_195_200),
            )
        } else {
            SmallOrder::new(
                Some(id(3)),
                Some(OrderKind::Buy),
                Some(OrderStatus::Pending),
                0,
                "EUR".to_string(),
                Some(10),
                Some(20),
                0,
                "face to face".to_string(),
                -5,
                key,
                None,
                None,
                Some(1_792_108_800),
                None,
            )
        };
        edit(&mut order);
        order
    }

    fn requested(ids: &[u128]) -> OrderInfoInput {
        OrderInfoInput::new(ids.iter().map(|n| id(*n))).unwrap()
    }

    fn node() -> PublicKey {
        PublicKey::from_hex(NODE).unwrap()
    }

    #[test]
    fn proposed_info_examples_conform_to_the_checked_in_schema() {
        let schema = read_json(INFO_FIXTURE);
        let examples = info_examples();
        assert_eq!(examples.len(), 7);
        for (name, exit, outcome) in examples {
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match outcome["error"]["code"].as_str() {
                None => 0,
                Some("invalid_arguments") => 2,
                Some(_) => 1,
            };
            assert_eq!(exit, expected, "{name}");
        }
    }

    #[test]
    fn info_examples_are_reproduced_by_the_owned_constructors() {
        let delivered = |daemon| ExecutionMeta::request(Publication::Published, daemon);
        let unsent =
            || ExecutionMeta::request(Publication::NotAttempted, DaemonAcceptance::Unknown);
        let reply = [small_order(1, |_| {}), small_order(3, |_| {})];
        let (data, warnings) = OrderInfoData::from_reply(&node(), &requested(&[1, 2, 3]), &reply);
        let subset = Envelope::ok(
            "order.info",
            data,
            warnings,
            delivered(DaemonAcceptance::Accepted),
        );
        let (data, warnings) = OrderInfoData::from_reply(&node(), &requested(&[1, 2]), &[]);
        let not_found = Envelope::ok(
            "order.info",
            data,
            warnings,
            delivered(DaemonAcceptance::Accepted),
        );
        let error = |code, meta| {
            Envelope::<OrderInfoData, ()>::error(Some("order.info"), code, Warnings::new(), meta)
        };
        let cases: Vec<(&str, Value, i32)> = vec![
            (
                "subset_success",
                to_value(&subset).unwrap(),
                subset.exit_code(),
            ),
            (
                "not_found_success",
                to_value(&not_found).unwrap(),
                not_found.exit_code(),
            ),
            (
                "node_refused",
                to_value(Envelope::<OrderInfoData, ()>::refused(
                    Some("order.info"),
                    "too_many_requests",
                    Warnings::new(),
                    delivered(DaemonAcceptance::Rejected),
                ))
                .unwrap(),
                1,
            ),
            (
                "reply_timeout",
                to_value(error(
                    ErrorCode::ReplyTimeout,
                    delivered(DaemonAcceptance::Unknown),
                ))
                .unwrap(),
                1,
            ),
            (
                "identity_missing",
                to_value(error(ErrorCode::IdentityMissing, unsent())).unwrap(),
                1,
            ),
            (
                "invalid_arguments",
                to_value(error(ErrorCode::InvalidArguments, unsent())).unwrap(),
                2,
            ),
            (
                "removed_spelling",
                to_value(Envelope::<OrderInfoData, ()>::error(
                    None,
                    ErrorCode::InvalidArguments,
                    Warnings::new(),
                    ExecutionMeta::not_executed(),
                ))
                .unwrap(),
                2,
            ),
        ];
        assert_eq!(cases.len(), info_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = info_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_info_variants_do_not_conform() {
        let schema = read_json(INFO_FIXTURE);
        let (_, base) = info_example("subset_success");
        let (_, refused) = info_example("node_refused");
        assert!(schema_check::conforms(&schema, &schema, &base));
        type Edit = Box<dyn Fn(&mut Value)>;
        let on_base: Vec<(&str, Edit)> = vec![
            (
                "number for a decimal string",
                Box::new(|o| o["data"]["orders"][0]["fiat_amount"] = json!(15)),
            ),
            (
                "null id",
                Box::new(|o| o["data"]["orders"][0]["id"] = Value::Null),
            ),
            (
                "negative zero",
                Box::new(|o| o["data"]["orders"][0]["sats_amount"] = json!("-0")),
            ),
            (
                "uppercase trade key",
                Box::new(|o| {
                    o["data"]["orders"][0]["seller_trade_pubkey"] = json!(NODE.to_uppercase())
                }),
            ),
            (
                "unknown status",
                Box::new(|o| o["data"]["orders"][0]["status"] = json!("uncertain")),
            ),
            (
                "unknown side",
                Box::new(|o| o["data"]["orders"][0]["side"] = json!("both")),
            ),
            (
                "offset time",
                Box::new(|o| {
                    o["data"]["orders"][0]["created_at"] = json!("2026-10-16T00:00:00+02:00")
                }),
            ),
            (
                "missing nullable key",
                Box::new(|o| {
                    o["data"]["orders"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("expires_at");
                }),
            ),
            (
                "not_returned entry that is not an id",
                Box::new(|o| o["data"]["not_returned"][0] = json!("not-a-uuid")),
            ),
            (
                "missing not_returned",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("not_returned");
                }),
            ),
            (
                "envelope status",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "read stages",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("not_applicable")),
            ),
            (
                "a local write",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "a snapshot",
                Box::new(|o| o["meta"]["snapshot"] = json!({})),
            ),
            (
                "another command",
                Box::new(|o| o["command"] = json!("order.list")),
            ),
        ];
        for (label, edit) in on_base {
            let mut outcome = base.clone();
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label}"
            );
        }
        let on_refused: Vec<(&str, Edit)> = vec![
            (
                "an uppercase reason",
                Box::new(|o| o["error"]["reason"] = json!("Too Many")),
            ),
            (
                "a numeric reason",
                Box::new(|o| o["error"]["reason"] = json!(429)),
            ),
            (
                "an accepted failure",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("accepted")),
            ),
            (
                "a code of another command",
                Box::new(|o| o["error"]["code"] = json!("output_not_supported")),
            ),
            (
                "data on an error",
                Box::new(|o| o["data"] = json!({"orders": []})),
            ),
        ];
        for (label, edit) in on_refused {
            let mut outcome = refused.clone();
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label}"
            );
        }
    }
    // --- The proposed outcomes of `order new` (spec 003) ---------------------

    const NEW_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/003-order-new-json/contracts/examples.json"
    );
    /// A second well-known public key, for the trade key.
    const TRADE: &str = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";

    fn new_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(NEW_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn new_example(name: &str) -> (i32, Value) {
        new_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    fn created(range: bool) -> OrderNewData {
        let input = OrderNewInput::validate(OrderRequest {
            side: OrderKind::Sell,
            currency: "usd".to_string(),
            fiat_amount: 100,
            fiat_amount_max: range.then_some(300),
            payment_method: "bank transfer".to_string(),
            sats: 0,
            premium: 2,
            invoice: None,
            expiration_days: 1,
            expires_in: None,
        })
        .unwrap();
        let mut order = input.small_order(1_792_108_800);
        order.id = Some(id(7));
        order.created_at = Some(1_792_108_800);
        OrderNewData::from_created(&node(), &order, 3, &PublicKey::from_hex(TRADE).unwrap())
            .unwrap()
            .0
    }

    /// The parked order of a node that asks the maker for a bond first.
    fn bonded() -> OrderNewData {
        let input = OrderNewInput::validate(OrderRequest {
            side: OrderKind::Sell,
            currency: "usd".to_string(),
            fiat_amount: 100,
            fiat_amount_max: None,
            payment_method: "bank transfer".to_string(),
            sats: 0,
            premium: 2,
            invoice: None,
            expiration_days: 1,
            expires_in: None,
        })
        .unwrap();
        let mut order = input.small_order(1_792_108_800);
        order.id = Some(id(7));
        order.created_at = Some(1_792_108_800);
        OrderNewData::from_bond(
            &node(),
            &order,
            "lnbcrt1bondinvoice",
            3,
            &PublicKey::from_hex(TRADE).unwrap(),
        )
        .unwrap()
        .0
    }

    type NewEnvelope = Envelope<OrderNewData, ()>;

    fn write(
        publication: Publication,
        acceptance: DaemonAcceptance,
        persistence: Persistence,
    ) -> ExecutionMeta<()> {
        ExecutionMeta::write(publication, acceptance, persistence)
    }

    fn unsent_write() -> ExecutionMeta<()> {
        write(
            Publication::NotAttempted,
            DaemonAcceptance::Unknown,
            Persistence::NotAttempted,
        )
    }

    fn new_cases() -> Vec<(&'static str, Value, i32)> {
        let accepted = || {
            write(
                Publication::Published,
                DaemonAcceptance::Accepted,
                Persistence::Applied,
            )
        };
        let error = |code, meta| NewEnvelope::error(Some("order.new"), code, Warnings::new(), meta);
        let uncertain = |code, data, meta| {
            NewEnvelope::uncertain("order.new", code, data, Warnings::new(), meta)
        };
        let ok = |data| NewEnvelope::ok("order.new", data, Warnings::new(), accepted());
        vec![
            ("created", to_value(ok(created(false))).unwrap(), 0),
            ("range_created", to_value(ok(created(true))).unwrap(), 0),
            (
                "node_refused",
                to_value(NewEnvelope::refused(
                    Some("order.new"),
                    "invalid_amount",
                    Warnings::new(),
                    write(
                        Publication::Published,
                        DaemonAcceptance::Rejected,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap(),
                1,
            ),
            ("bond_to_pay", to_value(ok(bonded())).unwrap(), 0),
            (
                "reply_timeout_uncertain",
                to_value(uncertain(
                    ErrorCode::ReplyTimeout,
                    None,
                    write(
                        Publication::Published,
                        DaemonAcceptance::Unknown,
                        Persistence::Applied,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "order_not_saved_uncertain",
                to_value(uncertain(
                    ErrorCode::OrderNotSaved,
                    Some(created(false)),
                    write(
                        Publication::Published,
                        DaemonAcceptance::Accepted,
                        Persistence::Failed,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "order_declined",
                to_value(error(ErrorCode::OrderDeclined, unsent_write())).unwrap(),
                1,
            ),
            (
                "identity_missing",
                to_value(error(ErrorCode::IdentityMissing, unsent_write())).unwrap(),
                1,
            ),
            (
                "yes_required",
                to_value(error(ErrorCode::InvalidArguments, unsent_write())).unwrap(),
                2,
            ),
            (
                "invalid_invoice",
                to_value(NewEnvelope::invalid_input(
                    Some("order.new"),
                    INVALID_INVOICE,
                    Warnings::new(),
                    unsent_write(),
                ))
                .unwrap(),
                2,
            ),
            (
                "removed_spelling",
                to_value(NewEnvelope::error(
                    None,
                    ErrorCode::InvalidArguments,
                    Warnings::new(),
                    ExecutionMeta::not_executed(),
                ))
                .unwrap(),
                2,
            ),
        ]
    }

    #[test]
    fn proposed_new_examples_conform_to_the_checked_in_schema() {
        let schema = read_json(NEW_FIXTURE);
        let examples = new_examples();
        assert_eq!(examples.len(), 11);
        for (name, exit, outcome) in examples {
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match (
                outcome["status"].as_str(),
                outcome["error"]["code"].as_str(),
            ) {
                (Some("ok"), _) => 0,
                (_, Some("invalid_arguments")) => 2,
                _ => 1,
            };
            assert_eq!(exit, expected, "{name}");
            // Only an ok outcome and an uncertain outcome after an accepted
            // reply carry data.
            assert_eq!(
                !outcome["data"].is_null(),
                matches!(
                    name.as_str(),
                    "created" | "range_created" | "bond_to_pay" | "order_not_saved_uncertain"
                ),
                "{name}"
            );
        }
    }

    #[test]
    fn new_examples_are_reproduced_by_the_owned_constructors() {
        let cases = new_cases();
        assert_eq!(cases.len(), new_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = new_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_new_variants_do_not_conform() {
        let schema = read_json(NEW_FIXTURE);
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "created",
                "a success with a failed save",
                Box::new(|o| o["meta"]["persistence"] = json!("failed")),
            ),
            (
                "created",
                "a success that is uncertain",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "created",
                "a numeric trade index",
                Box::new(|o| o["data"]["trade_index"] = json!(3)),
            ),
            (
                "created",
                "an uppercase trade key",
                Box::new(|o| o["data"]["trade_pubkey"] = json!(TRADE.to_uppercase())),
            ),
            (
                "created",
                "a missing trade key",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("trade_pubkey");
                }),
            ),
            (
                "created",
                "a read stage set",
                Box::new(|o| o["meta"]["publication"] = json!("not_applicable")),
            ),
            (
                "created",
                "another command",
                Box::new(|o| o["command"] = json!("order.info")),
            ),
            (
                "node_refused",
                "an uppercase reason",
                Box::new(|o| o["error"]["reason"] = json!("Invalid Amount")),
            ),
            (
                "node_refused",
                "an accepted refusal",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("accepted")),
            ),
            (
                "node_refused",
                // `applied` is allowed: a refusal for a stale index keeps it.
                "a refusal whose write failed",
                Box::new(|o| o["meta"]["persistence"] = json!("failed")),
            ),
            (
                "node_refused",
                "data on an error",
                Box::new(|o| o["data"] = json!({"order": null})),
            ),
            (
                "reply_timeout_uncertain",
                "a plain error status",
                Box::new(|o| o["status"] = json!("error")),
            ),
            (
                "reply_timeout_uncertain",
                "an accepted unknown reply",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("accepted")),
            ),
            (
                "reply_timeout_uncertain",
                "a code that is an error only",
                Box::new(|o| o["error"]["code"] = json!("order_declined")),
            ),
            (
                "reply_timeout_uncertain",
                "nothing published",
                Box::new(|o| o["meta"]["publication"] = json!("not_attempted")),
            ),
            (
                "order_not_saved_uncertain",
                "a saved persistence",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "order_declined",
                "a published declined order",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
            (
                "order_declined",
                "a code of a read command",
                Box::new(|o| o["error"]["code"] = json!("read_timeout")),
            ),
        ];
        for (case, label, edit) in variants {
            let (_, mut outcome) = new_example(case);
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }
    // --- The proposed outcomes of the takes (spec 004) -----------------------

    const TAKE_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/004-order-take-json/contracts/examples.json"
    );

    fn take_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(TAKE_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn take_example(name: &str) -> (i32, Value) {
        take_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    fn take_fixture(command: &str) -> Value {
        match command {
            "order.take-sell" => read_json(TAKE_SELL_FIXTURE),
            "order.take-buy" => read_json(TAKE_BUY_FIXTURE),
            other => panic!("no fixture for {other}"),
        }
    }

    fn taken_data(side: TakeSide, taken: &Taken, amount: Option<i64>) -> OrderTakeData {
        let input = OrderTakeInput::validate(TakeRequest {
            side,
            order: id(1),
            amount,
            invoice: None,
        })
        .unwrap();
        OrderTakeData::from_taken(
            &node(),
            &input,
            taken,
            3,
            &PublicKey::from_hex(TRADE).unwrap(),
        )
        .unwrap()
        .0
    }

    fn pending_order(status: OrderStatus) -> SmallOrder {
        let mut order = small_order(1, |_| {});
        order.status = Some(status);
        order.seller_trade_pubkey = None;
        order
    }

    type TakeEnvelope = Envelope<OrderTakeData, ()>;

    fn take_cases() -> Vec<(&'static str, Value, i32)> {
        let sell = "order.take-sell";
        let buy = "order.take-buy";
        let accepted = || {
            write(
                Publication::Published,
                DaemonAcceptance::Accepted,
                Persistence::Applied,
            )
        };
        let ok = |command, data| {
            to_value(TakeEnvelope::ok(command, data, Warnings::new(), accepted())).unwrap()
        };
        let error = |command, code, meta| {
            to_value(TakeEnvelope::error(
                Some(command),
                code,
                Warnings::new(),
                meta,
            ))
            .unwrap()
        };
        let add_invoice = Taken::AddInvoice(pending_order(OrderStatus::WaitingBuyerInvoice));
        let hold = Taken::PayInvoice(
            pending_order(OrderStatus::WaitingPayment),
            "lnbcrt1holdinvoice".to_string(),
        );
        vec![
            (
                "sell_add_invoice",
                ok(sell, taken_data(TakeSide::Sell, &add_invoice, None)),
                0,
            ),
            (
                "sell_waiting_for_the_seller",
                ok(
                    sell,
                    taken_data(TakeSide::Sell, &Taken::WaitingSellerToPay, Some(150)),
                ),
                0,
            ),
            (
                "buy_pay_hold_invoice",
                ok(buy, taken_data(TakeSide::Buy, &hold, None)),
                0,
            ),
            (
                "sell_node_refused",
                to_value(TakeEnvelope::refused(
                    Some(sell),
                    "invalid_order_status",
                    Warnings::new(),
                    write(
                        Publication::Published,
                        DaemonAcceptance::Rejected,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "sell_pay_bond_invoice",
                ok(
                    sell,
                    taken_data(
                        TakeSide::Sell,
                        &Taken::PayBond(
                            pending_order(OrderStatus::Pending),
                            "lnbcrt1bondinvoice".to_string(),
                        ),
                        None,
                    ),
                ),
                0,
            ),
            (
                "buy_reply_timeout_uncertain",
                to_value(TakeEnvelope::uncertain(
                    buy,
                    ErrorCode::ReplyTimeout,
                    None,
                    Warnings::new(),
                    write(
                        Publication::Published,
                        DaemonAcceptance::Unknown,
                        Persistence::Applied,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "buy_order_not_saved_uncertain",
                to_value(TakeEnvelope::uncertain(
                    buy,
                    ErrorCode::OrderNotSaved,
                    Some(taken_data(TakeSide::Buy, &hold, None)),
                    Warnings::new(),
                    write(
                        Publication::Published,
                        DaemonAcceptance::Accepted,
                        Persistence::Failed,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "sell_order_declined",
                error(sell, ErrorCode::OrderDeclined, unsent_write()),
                1,
            ),
            (
                "buy_identity_missing",
                error(buy, ErrorCode::IdentityMissing, unsent_write()),
                1,
            ),
            (
                "sell_yes_required",
                error(sell, ErrorCode::InvalidArguments, unsent_write()),
                2,
            ),
            (
                "sell_invalid_invoice",
                to_value(TakeEnvelope::invalid_input(
                    Some(sell),
                    INVALID_INVOICE,
                    Warnings::new(),
                    unsent_write(),
                ))
                .unwrap(),
                2,
            ),
            (
                "removed_spelling",
                to_value(TakeEnvelope::error(
                    None,
                    ErrorCode::InvalidArguments,
                    Warnings::new(),
                    ExecutionMeta::not_executed(),
                ))
                .unwrap(),
                2,
            ),
        ]
    }

    #[test]
    fn proposed_take_examples_conform_to_the_checked_in_schemas() {
        let examples = take_examples();
        assert_eq!(examples.len(), 12);
        for (name, exit, outcome) in examples {
            if outcome["command"].is_null() {
                continue; // no command resolved: both schemas allow it
            }
            let schema = take_fixture(outcome["command"].as_str().unwrap());
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match (
                outcome["status"].as_str(),
                outcome["error"]["code"].as_str(),
            ) {
                (Some("ok"), _) => 0,
                (_, Some("invalid_arguments")) => 2,
                _ => 1,
            };
            assert_eq!(exit, expected, "{name}");
            assert_eq!(
                !outcome["data"].is_null(),
                matches!(
                    name.as_str(),
                    "sell_add_invoice"
                        | "sell_waiting_for_the_seller"
                        | "buy_pay_hold_invoice"
                        | "sell_pay_bond_invoice"
                        | "buy_order_not_saved_uncertain"
                ),
                "{name}"
            );
        }
        // The removed spelling conforms to both schemas.
        let (_, removed) = take_example("removed_spelling");
        for command in ["order.take-sell", "order.take-buy"] {
            let schema = take_fixture(command);
            assert!(schema_check::conforms(&schema, &schema, &removed));
        }
    }

    #[test]
    fn take_examples_are_reproduced_by_the_owned_constructors() {
        let cases = take_cases();
        assert_eq!(cases.len(), take_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = take_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_take_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "sell_add_invoice",
                "an unknown next step",
                Box::new(|o| o["data"]["next_step"] = json!("pay_now")),
            ),
            (
                "sell_add_invoice",
                "a numeric trade index",
                Box::new(|o| o["data"]["trade_index"] = json!(3)),
            ),
            (
                "sell_add_invoice",
                "an uppercase trade key",
                Box::new(|o| o["data"]["trade_pubkey"] = json!(TRADE.to_uppercase())),
            ),
            (
                "sell_add_invoice",
                "a missing payment_request key",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("payment_request");
                }),
            ),
            (
                "sell_add_invoice",
                "an unknown side",
                Box::new(|o| o["data"]["side"] = json!("both")),
            ),
            (
                "sell_add_invoice",
                "a bad order id",
                Box::new(|o| o["data"]["order_id"] = json!("not-a-uuid")),
            ),
            (
                "sell_add_invoice",
                "a failed save on success",
                Box::new(|o| o["meta"]["persistence"] = json!("failed")),
            ),
            (
                "sell_add_invoice",
                "a success that is uncertain",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "sell_add_invoice",
                "the other command",
                Box::new(|o| o["command"] = json!("order.take-buy")),
            ),
            (
                "sell_node_refused",
                "an uppercase reason",
                Box::new(|o| o["error"]["reason"] = json!("Invalid Status")),
            ),
            (
                "sell_node_refused",
                "an accepted refusal",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("accepted")),
            ),
            (
                "sell_node_refused",
                "data on an error",
                Box::new(|o| o["data"] = json!({"order_id": null})),
            ),
            (
                "buy_reply_timeout_uncertain",
                "a plain error status",
                Box::new(|o| o["status"] = json!("error")),
            ),
            (
                "buy_reply_timeout_uncertain",
                "a code that is an error only",
                Box::new(|o| o["error"]["code"] = json!("order_declined")),
            ),
            (
                "buy_reply_timeout_uncertain",
                "nothing published",
                Box::new(|o| o["meta"]["publication"] = json!("not_attempted")),
            ),
            (
                "buy_order_not_saved_uncertain",
                "a saved persistence",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "sell_order_declined",
                "a published declined take",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
        ];
        for (case, label, edit) in variants {
            let (_, mut outcome) = take_example(case);
            let schema = take_fixture(outcome["command"].as_str().unwrap());
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            // The edit that swaps the command must be judged by the original schema.
            let schema = if label == "the other command" {
                take_fixture("order.take-sell")
            } else {
                schema
            };
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }

    // --- Trade steps (spec 007) ---

    use mostro_client::outcome::step::{StepData, StepInput, StepKind};

    const STEP_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/007-trade-steps-json/contracts/examples.json"
    );

    fn step_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(STEP_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn step_example(name: &str) -> (i32, Value) {
        step_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    fn step_fixture(command: &str) -> Value {
        match command {
            "order.fiat-sent" => read_json(FIAT_SENT_FIXTURE),
            "order.release" => read_json(RELEASE_FIXTURE),
            "order.cancel" => read_json(CANCEL_FIXTURE),
            other => panic!("no fixture for {other}"),
        }
    }

    fn step_data(kind: StepKind, action: Action, index: Option<i64>) -> Value {
        let data = StepData::from_reply(
            &node(),
            &StepInput::new(kind, id(1)),
            (&action, &None, Some(id(1))),
            &PublicKey::from_hex(TRADE).unwrap(),
            index,
        )
        .unwrap()
        .0;
        to_value(data).unwrap()
    }

    fn step_cases() -> Vec<(&'static str, Value, i32)> {
        let fiat = "order.fiat-sent";
        let release = "order.release";
        let cancel = "order.cancel";
        let accepted = |persistence| {
            write(
                Publication::Published,
                DaemonAcceptance::Accepted,
                persistence,
            )
        };
        let ok = |command, data: Value, warnings, meta| {
            to_value(Envelope::<Value, ()>::ok(command, data, warnings, meta)).unwrap()
        };
        let refused = |command, reason| {
            to_value(Envelope::<Value, ()>::refused(
                Some(command),
                reason,
                Warnings::new(),
                write(
                    Publication::Published,
                    DaemonAcceptance::Rejected,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap()
        };
        let uncertain = |command, code| {
            to_value(Envelope::<Value, ()>::uncertain(
                command,
                code,
                None,
                Warnings::new(),
                write(
                    Publication::Published,
                    DaemonAcceptance::Unknown,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap()
        };
        let error = |command, code| {
            to_value(Envelope::<Value, ()>::error(
                Some(command),
                code,
                Warnings::new(),
                unsent_write(),
            ))
            .unwrap()
        };
        let mut stale = Warnings::new();
        stale.add(mostro_client::outcome::WarningCode::LocalStatusStale);
        vec![
            (
                "fiat_sent",
                ok(
                    fiat,
                    step_data(StepKind::FiatSent, Action::FiatSentOk, None),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            (
                "fiat_sent_range_key_sent",
                ok(
                    fiat,
                    step_data(StepKind::FiatSent, Action::FiatSentOk, Some(4)),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            (
                "release_released",
                ok(
                    release,
                    step_data(StepKind::Release, Action::HoldInvoicePaymentSettled, None),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            (
                "release_range_continued",
                ok(
                    release,
                    step_data(
                        StepKind::Release,
                        Action::HoldInvoicePaymentSettled,
                        Some(4),
                    ),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            (
                "release_stale_status",
                ok(
                    release,
                    step_data(StepKind::Release, Action::HoldInvoicePaymentSettled, None),
                    stale,
                    accepted(Persistence::Failed),
                ),
                0,
            ),
            (
                "cancel_canceled",
                ok(
                    cancel,
                    step_data(StepKind::Cancel, Action::Canceled, None),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            (
                "cancel_requested",
                ok(
                    cancel,
                    step_data(
                        StepKind::Cancel,
                        Action::CooperativeCancelInitiatedByYou,
                        None,
                    ),
                    Warnings::new(),
                    accepted(Persistence::NotAttempted),
                ),
                0,
            ),
            (
                "cancel_accepted",
                ok(
                    cancel,
                    step_data(StepKind::Cancel, Action::CooperativeCancelAccepted, None),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            ("fiat_sent_node_refused", refused(fiat, "invalid_pubkey"), 1),
            ("release_node_refused", refused(release, "invalid_peer"), 1),
            (
                "cancel_node_refused",
                refused(cancel, "order_already_canceled"),
                1,
            ),
            (
                "release_reply_timeout_uncertain",
                uncertain(release, ErrorCode::ReplyTimeout),
                1,
            ),
            (
                "cancel_reply_malformed_uncertain",
                uncertain(cancel, ErrorCode::ReplyMalformed),
                1,
            ),
            (
                "release_unknown",
                error(release, ErrorCode::OrderUnknown),
                1,
            ),
            (
                "cancel_declined",
                error(cancel, ErrorCode::OrderDeclined),
                1,
            ),
            (
                "fiat_sent_yes_required",
                error(fiat, ErrorCode::InvalidArguments),
                2,
            ),
            (
                "removed_spelling",
                to_value(Envelope::<Value, ()>::error(
                    None,
                    ErrorCode::InvalidArguments,
                    Warnings::new(),
                    ExecutionMeta::not_executed(),
                ))
                .unwrap(),
                2,
            ),
        ]
    }

    #[test]
    fn proposed_step_examples_conform_to_the_checked_in_schemas() {
        let examples = step_examples();
        assert_eq!(examples.len(), 17);
        for (name, exit, outcome) in examples {
            if outcome["command"].is_null() {
                continue; // no command resolved: every schema allows it
            }
            let schema = step_fixture(outcome["command"].as_str().unwrap());
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match (
                outcome["status"].as_str(),
                outcome["error"]["code"].as_str(),
            ) {
                (Some("ok"), _) => 0,
                (_, Some("invalid_arguments")) => 2,
                _ => 1,
            };
            assert_eq!(exit, expected, "{name}");
            assert_eq!(
                !outcome["data"].is_null(),
                outcome["status"] == "ok",
                "{name}: data exactly on success"
            );
        }
        let (_, removed) = step_example("removed_spelling");
        for command in ["order.fiat-sent", "order.release", "order.cancel"] {
            let schema = step_fixture(command);
            assert!(schema_check::conforms(&schema, &schema, &removed));
        }
    }

    #[test]
    fn step_examples_are_reproduced_by_the_owned_constructors() {
        let cases = step_cases();
        assert_eq!(cases.len(), step_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = step_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_step_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "fiat_sent",
                "an unknown result",
                Box::new(|o| o["data"]["result"] = json!("paid")),
            ),
            (
                "fiat_sent",
                "an unknown next step",
                Box::new(|o| o["data"]["next_step"] = json!("rate")),
            ),
            (
                "fiat_sent",
                "a bad order id",
                Box::new(|o| o["data"]["order_id"] = json!("not-a-uuid")),
            ),
            (
                "fiat_sent",
                "an uppercase trade key",
                Box::new(|o| o["data"]["trade_pubkey"] = json!(TRADE.to_uppercase())),
            ),
            (
                "fiat_sent",
                "a numeric index",
                Box::new(|o| o["data"]["next_trade_index"] = json!(3)),
            ),
            (
                "fiat_sent",
                "a missing index key",
                Box::new(|o| {
                    o["data"]
                        .as_object_mut()
                        .unwrap()
                        .remove("next_trade_index");
                }),
            ),
            (
                "fiat_sent",
                "a success that is uncertain",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "fiat_sent",
                "a rejected success",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("rejected")),
            ),
            (
                "fiat_sent",
                "another command's name",
                Box::new(|o| o["command"] = json!("order.cancel")),
            ),
            (
                "release_stale_status",
                "an unknown warning",
                Box::new(|o| o["warnings"][0]["code"] = json!("trade_keys_limited")),
            ),
            (
                "release_range_continued",
                "a numeric index",
                Box::new(|o| o["data"]["next_trade_index"] = json!(4)),
            ),
            (
                "release_node_refused",
                "an uppercase reason",
                Box::new(|o| o["error"]["reason"] = json!("Invalid Peer")),
            ),
            (
                "release_node_refused",
                "an accepted refusal",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("accepted")),
            ),
            (
                "release_node_refused",
                "data on an error",
                Box::new(|o| o["data"] = json!({"order_id": null})),
            ),
            (
                "release_reply_timeout_uncertain",
                "a plain error status",
                Box::new(|o| o["status"] = json!("error")),
            ),
            (
                "release_reply_timeout_uncertain",
                "a code that is an error only",
                Box::new(|o| o["error"]["code"] = json!("order_declined")),
            ),
            (
                "release_reply_timeout_uncertain",
                "nothing published",
                Box::new(|o| o["meta"]["publication"] = json!("not_attempted")),
            ),
            (
                "cancel_declined",
                "a published declined step",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
        ];
        for (case, label, edit) in variants {
            let (_, mut outcome) = step_example(case);
            let schema = step_fixture(outcome["command"].as_str().unwrap());
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }

    // --- Order sync (spec 008) ---

    use mostro_client::outcome::sync::{AdoptedOrder, SyncData, SyncInput};

    const SYNC_COMMAND: &str = "order.sync";
    const SYNC_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/008-range-children/contracts/examples.json"
    );

    fn sync_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(SYNC_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn sync_example(name: &str) -> (i32, Value) {
        sync_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    /// A pending range child of 10 to 60, announced to the key of index 3.
    fn adopted_child() -> AdoptedOrder {
        let mut order = pending_order(OrderStatus::Pending);
        order.min_amount = Some(10);
        order.max_amount = Some(60);
        order.fiat_amount = 0;
        order.amount = 0;
        AdoptedOrder::new(
            id(5),
            &order,
            (3, &PublicKey::from_hex(TRADE).unwrap()),
            &mut Warnings::new(),
        )
    }

    fn sync_data(
        adopted: Vec<AdoptedOrder>,
        read: usize,
        truncated: bool,
        keys: (usize, usize),
    ) -> Value {
        let input = SyncInput::validate(Some(60)).unwrap();
        to_value(SyncData::new(
            &node(),
            &input,
            adopted,
            (read, truncated),
            keys,
        ))
        .unwrap()
    }

    fn sync_cases() -> Vec<(&'static str, Value, i32)> {
        let ok = |data: Value, warnings, snapshot, persistence| {
            to_value(MessageEnvelope::ok(
                SYNC_COMMAND,
                data,
                warnings,
                ExecutionMeta::sync(Some(snapshot), persistence),
            ))
            .unwrap()
        };
        let uncertain = |data: Value| {
            to_value(MessageEnvelope::uncertain(
                SYNC_COMMAND,
                ErrorCode::OrderNotSaved,
                Some(data),
                Warnings::new(),
                ExecutionMeta::sync(Some(read_snapshot(2, 2)), Persistence::Failed),
            ))
            .unwrap()
        };
        let error = |code, meta| {
            to_value(MessageEnvelope::error(
                Some(SYNC_COMMAND),
                code,
                Warnings::new(),
                meta,
            ))
            .unwrap()
        };
        let before_reading = || ExecutionMeta::read(None);
        let mut limited = Warnings::new();
        limited.add(mostro_client::outcome::WarningCode::TradeKeysLimited);
        vec![
            (
                "child_order_added",
                ok(
                    sync_data(vec![adopted_child()], 4, false, (3, 0)),
                    Warnings::new(),
                    read_snapshot(2, 4),
                    Persistence::Applied,
                ),
                0,
            ),
            (
                "nothing_to_add",
                ok(
                    sync_data(Vec::new(), 4, false, (3, 0)),
                    Warnings::new(),
                    read_snapshot(2, 4),
                    Persistence::NotAttempted,
                ),
                0,
            ),
            (
                "more_messages_and_keys_than_were_read",
                ok(
                    sync_data(vec![adopted_child()], 50, true, (200, 50)),
                    limited,
                    read_snapshot(2, 50),
                    Persistence::Applied,
                ),
                0,
            ),
            (
                "one_order_saved_and_one_not",
                uncertain(sync_data(vec![adopted_child()], 2, false, (3, 0))),
                1,
            ),
            (
                "identity_missing",
                error(ErrorCode::IdentityMissing, before_reading()),
                1,
            ),
            (
                "store_unwritable",
                error(ErrorCode::StoreUnwritable, before_reading()),
                1,
            ),
            (
                "store_unreadable_after_the_keys",
                error(
                    ErrorCode::StoreUnreadable,
                    ExecutionMeta::sync(None, Persistence::NotAttempted),
                ),
                1,
            ),
            (
                "relays_unreachable",
                error(
                    ErrorCode::RelayConnectionFailed,
                    ExecutionMeta::sync(None, Persistence::NotAttempted),
                ),
                1,
            ),
            (
                "read_timeout",
                error(
                    ErrorCode::ReadTimeout,
                    ExecutionMeta::sync(Some(read_snapshot(0, 0)), Persistence::NotAttempted),
                ),
                1,
            ),
            (
                "window_out_of_range",
                error(ErrorCode::InvalidArguments, before_reading()),
                2,
            ),
        ]
    }

    #[test]
    fn proposed_sync_examples_conform_to_the_checked_in_schema() {
        let schema = read_json(SYNC_FIXTURE);
        let examples = sync_examples();
        assert_eq!(examples.len(), 10);
        for (name, exit, outcome) in examples {
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match (
                outcome["status"].as_str(),
                outcome["error"]["code"].as_str(),
            ) {
                (Some("ok"), _) => 0,
                (_, Some("invalid_arguments")) => 2,
                _ => 1,
            };
            assert_eq!(exit, expected, "{name}");
            // Data on a success, and on the one uncertain outcome that saved rows.
            assert_eq!(
                !outcome["data"].is_null(),
                matches!(
                    name.as_str(),
                    "child_order_added"
                        | "nothing_to_add"
                        | "more_messages_and_keys_than_were_read"
                        | "one_order_saved_and_one_not"
                ),
                "{name}"
            );
        }
    }

    #[test]
    fn sync_examples_are_reproduced_by_the_owned_constructors() {
        let cases = sync_cases();
        assert_eq!(cases.len(), sync_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = sync_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_sync_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "child_order_added",
                "a numeric trade index",
                Box::new(|o| o["data"]["adopted"][0]["trade_index"] = json!(3)),
            ),
            (
                "child_order_added",
                "an uppercase trade key",
                Box::new(|o| o["data"]["adopted"][0]["trade_pubkey"] = json!(TRADE.to_uppercase())),
            ),
            (
                "child_order_added",
                "an order without an id",
                Box::new(|o| {
                    o["data"]["adopted"][0]["order"]
                        .as_object_mut()
                        .unwrap()
                        .remove("id");
                }),
            ),
            (
                "child_order_added",
                "a count that is a number",
                Box::new(|o| o["data"]["messages_read"] = json!(4)),
            ),
            (
                "child_order_added",
                "a missing truncation flag",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("truncated");
                }),
            ),
            (
                "child_order_added",
                "a published request",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
            (
                "child_order_added",
                "rows saved but not applied",
                Box::new(|o| o["meta"]["persistence"] = json!("not_applicable")),
            ),
            (
                "child_order_added",
                "no snapshot",
                Box::new(|o| o["meta"]["snapshot"] = Value::Null),
            ),
            (
                "nothing_to_add",
                "a failed write on a success",
                Box::new(|o| o["meta"]["persistence"] = json!("failed")),
            ),
            (
                "more_messages_and_keys_than_were_read",
                "a warning of another command",
                Box::new(|o| o["warnings"][0]["code"] = json!("local_status_stale")),
            ),
            (
                "one_order_saved_and_one_not",
                "a plain error status",
                Box::new(|o| o["status"] = json!("error")),
            ),
            (
                "one_order_saved_and_one_not",
                "a code that is an error only",
                Box::new(|o| o["error"]["code"] = json!("identity_missing")),
            ),
            (
                "one_order_saved_and_one_not",
                "persistence that did not fail",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "identity_missing",
                "a node refusal",
                Box::new(|o| o["error"]["code"] = json!("node_refused")),
            ),
            (
                "read_timeout",
                "data on an error",
                Box::new(|o| o["data"] = json!({"adopted": []})),
            ),
        ];
        let schema = read_json(SYNC_FIXTURE);
        for (case, label, edit) in variants {
            let (_, mut outcome) = sync_example(case);
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }

    // --- Invoice commands (spec 005) ---

    const INVOICE_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/005-invoice-json/contracts/examples.json"
    );

    fn invoice_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(INVOICE_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn invoice_example(name: &str) -> (i32, Value) {
        invoice_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    fn invoice_fixture(command: &str) -> Value {
        match command {
            "order.add-invoice" => read_json(ADD_INVOICE_FIXTURE),
            "bond.add-invoice" => read_json(BOND_INVOICE_FIXTURE),
            other => panic!("no fixture for {other}"),
        }
    }

    fn invoice_input(kind: InvoiceKind) -> InvoiceInput {
        InvoiceInput::validate(InvoiceRequest {
            kind,
            order: id(1),
            invoice: "lnbcrt1buyerinvoice".to_string(),
        })
        .unwrap()
    }

    fn order_invoice_data(answered: &Answered) -> OrderInvoiceData {
        OrderInvoiceData::from_answer(
            &node(),
            &invoice_input(InvoiceKind::Order),
            answered,
            &PublicKey::from_hex(TRADE).unwrap(),
        )
        .unwrap()
        .0
    }

    fn bond_invoice_data() -> BondInvoiceData {
        let mut share = pending_order(OrderStatus::Success);
        share.amount = 5_000;
        BondInvoiceData::from_ack(
            &node(),
            &invoice_input(InvoiceKind::Bond),
            &share,
            &PublicKey::from_hex(TRADE).unwrap(),
        )
        .unwrap()
    }

    fn invoice_cases() -> Vec<(&'static str, Value, i32)> {
        let order = "order.add-invoice";
        let bond = "bond.add-invoice";
        let invalid_invoice = |command| {
            to_value(Envelope::<Value, ()>::invalid_input(
                Some(command),
                INVALID_INVOICE,
                Warnings::new(),
                write(
                    Publication::NotAttempted,
                    DaemonAcceptance::Unknown,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap()
        };
        let meta =
            |publication, acceptance, persistence| write(publication, acceptance, persistence);
        let accepted = |persistence| {
            meta(
                Publication::Published,
                DaemonAcceptance::Accepted,
                persistence,
            )
        };
        let ok = |command, data: Value, warnings, meta| {
            to_value(Envelope::<Value, ()>::ok(command, data, warnings, meta)).unwrap()
        };
        let refused = |command, reason| {
            to_value(Envelope::<Value, ()>::refused(
                Some(command),
                reason,
                Warnings::new(),
                meta(
                    Publication::Published,
                    DaemonAcceptance::Rejected,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap()
        };
        let uncertain = |command, code| {
            to_value(Envelope::<Value, ()>::uncertain(
                command,
                code,
                None,
                Warnings::new(),
                meta(
                    Publication::Published,
                    DaemonAcceptance::Unknown,
                    Persistence::NotAttempted,
                ),
            ))
            .unwrap()
        };
        let error = |command, code| {
            to_value(Envelope::<Value, ()>::error(
                Some(command),
                code,
                Warnings::new(),
                unsent_write(),
            ))
            .unwrap()
        };
        let data = |data: &OrderInvoiceData| to_value(data).unwrap();
        let accepted_payment =
            Answered::PaymentAccepted(Box::new(pending_order(OrderStatus::Active)));
        let mut stale = Warnings::new();
        stale.add(mostro_client::outcome::WarningCode::LocalStatusStale);
        vec![
            (
                "order_waiting_seller_payment",
                ok(
                    order,
                    data(&order_invoice_data(&Answered::WaitingSellerToPay)),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            (
                "order_payment_accepted",
                ok(
                    order,
                    data(&order_invoice_data(&accepted_payment)),
                    Warnings::new(),
                    accepted(Persistence::Applied),
                ),
                0,
            ),
            (
                "order_invoice_updated",
                ok(
                    order,
                    data(&order_invoice_data(&Answered::InvoiceUpdated)),
                    Warnings::new(),
                    accepted(Persistence::NotAttempted),
                ),
                0,
            ),
            (
                "order_stale_status",
                ok(
                    order,
                    data(&order_invoice_data(&Answered::WaitingSellerToPay)),
                    stale,
                    accepted(Persistence::Failed),
                ),
                0,
            ),
            ("order_node_refused", refused(order, "invalid_peer"), 1),
            (
                "order_reply_timeout_uncertain",
                uncertain(order, ErrorCode::ReplyTimeout),
                1,
            ),
            (
                "order_reply_malformed_uncertain",
                uncertain(order, ErrorCode::ReplyMalformed),
                1,
            ),
            ("order_unknown", error(order, ErrorCode::OrderUnknown), 1),
            ("order_declined", error(order, ErrorCode::OrderDeclined), 1),
            (
                "order_yes_required",
                error(order, ErrorCode::InvalidArguments),
                2,
            ),
            ("order_invalid_invoice", invalid_invoice(order), 2),
            ("bond_invalid_invoice", invalid_invoice(bond), 2),
            (
                "bond_invoice_accepted",
                ok(
                    bond,
                    to_value(bond_invoice_data()).unwrap(),
                    Warnings::new(),
                    accepted(Persistence::NotAttempted),
                ),
                0,
            ),
            ("bond_node_refused", refused(bond, "invalid_invoice"), 1),
            (
                "bond_reply_timeout_uncertain",
                uncertain(bond, ErrorCode::ReplyTimeout),
                1,
            ),
            ("bond_unknown", error(bond, ErrorCode::OrderUnknown), 1),
            (
                "removed_spelling",
                to_value(Envelope::<Value, ()>::error(
                    None,
                    ErrorCode::InvalidArguments,
                    Warnings::new(),
                    ExecutionMeta::not_executed(),
                ))
                .unwrap(),
                2,
            ),
        ]
    }

    #[test]
    fn proposed_invoice_examples_conform_to_the_checked_in_schemas() {
        let examples = invoice_examples();
        assert_eq!(examples.len(), 17);
        for (name, exit, outcome) in examples {
            if outcome["command"].is_null() {
                continue; // no command resolved: both schemas allow it
            }
            let schema = invoice_fixture(outcome["command"].as_str().unwrap());
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match (
                outcome["status"].as_str(),
                outcome["error"]["code"].as_str(),
            ) {
                (Some("ok"), _) => 0,
                (_, Some("invalid_arguments")) => 2,
                _ => 1,
            };
            assert_eq!(exit, expected, "{name}");
            assert_eq!(
                !outcome["data"].is_null(),
                matches!(
                    name.as_str(),
                    "order_waiting_seller_payment"
                        | "order_payment_accepted"
                        | "order_invoice_updated"
                        | "order_stale_status"
                        | "bond_invoice_accepted"
                ),
                "{name}"
            );
        }
        let (_, removed) = invoice_example("removed_spelling");
        for command in ["order.add-invoice", "bond.add-invoice"] {
            let schema = invoice_fixture(command);
            assert!(schema_check::conforms(&schema, &schema, &removed));
        }
    }

    #[test]
    fn invoice_examples_are_reproduced_by_the_owned_constructors() {
        let cases = invoice_cases();
        assert_eq!(cases.len(), invoice_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = invoice_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_invoice_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "order_waiting_seller_payment",
                "an unknown result",
                Box::new(|o| o["data"]["result"] = json!("paid")),
            ),
            (
                "order_waiting_seller_payment",
                "an uppercase trade key",
                Box::new(|o| o["data"]["trade_pubkey"] = json!(TRADE.to_uppercase())),
            ),
            (
                "order_waiting_seller_payment",
                "a bad order id",
                Box::new(|o| o["data"]["order_id"] = json!("not-a-uuid")),
            ),
            (
                "order_waiting_seller_payment",
                "a missing order key",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("order");
                }),
            ),
            (
                "order_waiting_seller_payment",
                "a success that is uncertain",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "order_waiting_seller_payment",
                "a rejected success",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("rejected")),
            ),
            (
                "order_node_refused",
                "an uppercase reason",
                Box::new(|o| o["error"]["reason"] = json!("Invalid Peer")),
            ),
            (
                "order_node_refused",
                "an accepted refusal",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("accepted")),
            ),
            (
                "order_node_refused",
                "data on an error",
                Box::new(|o| o["data"] = json!({"order_id": null})),
            ),
            (
                "order_reply_timeout_uncertain",
                "a plain error status",
                Box::new(|o| o["status"] = json!("error")),
            ),
            (
                "order_reply_timeout_uncertain",
                "a code that is an error only",
                Box::new(|o| o["error"]["code"] = json!("order_declined")),
            ),
            (
                "order_reply_timeout_uncertain",
                "nothing published",
                Box::new(|o| o["meta"]["publication"] = json!("not_attempted")),
            ),
            (
                "order_declined",
                "a published declined invoice",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
            (
                "bond_invoice_accepted",
                "a local write",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "bond_invoice_accepted",
                "a numeric share",
                Box::new(|o| o["data"]["counterparty_share_sats"] = json!(5000)),
            ),
            (
                "bond_invoice_accepted",
                "an unknown result",
                Box::new(|o| o["data"]["result"] = json!("paid")),
            ),
            (
                "bond_unknown",
                "a save failure on a bond, which saves nothing",
                Box::new(|o| o["error"]["code"] = json!("order_not_saved")),
            ),
        ];
        for (case, label, edit) in variants {
            let (_, mut outcome) = invoice_example(case);
            let schema = invoice_fixture(outcome["command"].as_str().unwrap());
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }

    // --- Message list (spec 006) ---

    const MESSAGE_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/006-message-list-json/contracts/examples.json"
    );
    const MESSAGE_LIST: &str = "message.list";

    fn message_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(MESSAGE_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn message_example(name: &str) -> (i32, Value) {
        message_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    fn message_record(
        action: Action,
        order: Option<uuid::Uuid>,
        payload: Option<Payload>,
        minute: u64,
    ) -> MessageRecord {
        let message = Message::new_order(order, Some(77), None, action, payload);
        MessageRecord::from_message(
            &EventId::from_byte_array([7; 32]),
            1_792_108_800 + minute * 60,
            &message,
            Some(3),
            &PublicKey::from_hex(TRADE).unwrap(),
            &mut Warnings::new(),
        )
    }

    fn message_records() -> Vec<MessageRecord> {
        let parked = pending_order(OrderStatus::Pending);
        let waiting = pending_order(OrderStatus::WaitingBuyerInvoice);
        vec![
            message_record(
                Action::PayBondInvoice,
                Some(id(1)),
                Some(Payload::PaymentRequest(
                    Some(parked),
                    "lnbcrt1bondinvoice".into(),
                    None,
                )),
                1,
            ),
            message_record(
                Action::AddInvoice,
                Some(id(1)),
                Some(Payload::Order(waiting)),
                2,
            ),
            message_record(Action::WaitingSellerToPay, Some(id(1)), None, 3),
            message_record(
                Action::CantDo,
                None,
                Some(Payload::CantDo(Some(CantDoReason::InvalidPeer))),
                4,
            ),
        ]
    }

    fn message_data(
        records: Vec<MessageRecord>,
        truncated: bool,
        keys: (usize, usize),
    ) -> MessageListData {
        let input = MessageListInput::validate(Some(60), None).unwrap();
        MessageListData::new(&node(), &input, records, truncated, keys)
    }

    fn read_snapshot(completed: usize, events: usize) -> Value {
        json!({
            "since": "2026-10-09T00:00:00Z", "until": null,
            "observed_at": "2026-10-16T00:00:00Z",
            "per_relay_limit": 50, "max_unique_events": 10000,
            "configured_relays": 2, "completed_relays": completed,
            "received_events": events,
            "completion_policy": "all_relays_eose", "complete_history": false,
        })
    }

    type MessageEnvelope = Envelope<Value, Value>;

    fn message_cases() -> Vec<(&'static str, Value, i32)> {
        let ok = |data: MessageListData, warnings, snapshot| {
            to_value(MessageEnvelope::ok(
                MESSAGE_LIST,
                to_value(data).unwrap(),
                warnings,
                ExecutionMeta::read(Some(snapshot)),
            ))
            .unwrap()
        };
        let error = |code, snapshot| {
            to_value(MessageEnvelope::error(
                Some(MESSAGE_LIST),
                code,
                Warnings::new(),
                ExecutionMeta::read(snapshot),
            ))
            .unwrap()
        };
        let mut limited = Warnings::new();
        limited.add(mostro_client::outcome::WarningCode::TradeKeysLimited);
        vec![
            (
                "messages_of_a_trade",
                ok(
                    message_data(message_records(), false, (3, 0)),
                    Warnings::new(),
                    read_snapshot(2, 4),
                ),
                0,
            ),
            (
                "no_messages_in_the_window",
                ok(
                    message_data(Vec::new(), false, (3, 0)),
                    Warnings::new(),
                    read_snapshot(2, 0),
                ),
                0,
            ),
            (
                "more_messages_and_keys_than_were_read",
                ok(
                    message_data(message_records(), true, (200, 50)),
                    limited,
                    read_snapshot(2, 50),
                ),
                0,
            ),
            (
                "identity_missing",
                error(ErrorCode::IdentityMissing, None),
                1,
            ),
            (
                "store_unreadable",
                error(ErrorCode::StoreUnreadable, None),
                1,
            ),
            ("order_unknown", error(ErrorCode::OrderUnknown, None), 1),
            (
                "relays_unreachable",
                error(ErrorCode::RelayConnectionFailed, None),
                1,
            ),
            (
                "read_timeout",
                error(ErrorCode::ReadTimeout, Some(read_snapshot(0, 0))),
                1,
            ),
            (
                "window_out_of_range",
                error(ErrorCode::InvalidArguments, None),
                2,
            ),
            (
                "removed_getdm",
                to_value(Envelope::<Value, ()>::error(
                    None,
                    ErrorCode::InvalidArguments,
                    Warnings::new(),
                    ExecutionMeta::not_executed(),
                ))
                .unwrap(),
                2,
            ),
        ]
    }

    #[test]
    fn proposed_message_examples_conform_to_the_checked_in_schema() {
        let schema = read_json(MESSAGE_LIST_FIXTURE);
        let examples = message_examples();
        assert_eq!(examples.len(), 10);
        for (name, exit, outcome) in examples {
            if outcome["command"].is_null() {
                continue; // no command resolved: the schema allows it
            }
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match (
                outcome["status"].as_str(),
                outcome["error"]["code"].as_str(),
            ) {
                (Some("ok"), _) => 0,
                (_, Some("invalid_arguments")) => 2,
                _ => 1,
            };
            assert_eq!(exit, expected, "{name}");
            assert_eq!(
                !outcome["data"].is_null(),
                name.starts_with("messages_")
                    || name == "no_messages_in_the_window"
                    || name.starts_with("more_"),
                "{name}"
            );
        }
        let (_, removed) = message_example("removed_getdm");
        assert!(schema_check::conforms(&schema, &schema, &removed));
    }

    #[test]
    fn message_examples_are_reproduced_by_the_owned_constructors() {
        let cases = message_cases();
        assert_eq!(cases.len(), message_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = message_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_message_list_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "messages_of_a_trade",
                "an unknown next step",
                Box::new(|o| o["data"]["messages"][0]["next_step"] = json!("wait")),
            ),
            (
                "messages_of_a_trade",
                "an unknown payload kind",
                Box::new(|o| o["data"]["messages"][0]["payload"]["kind"] = json!("secret")),
            ),
            (
                "messages_of_a_trade",
                "a numeric trade index",
                Box::new(|o| o["data"]["messages"][0]["trade_index"] = json!(3)),
            ),
            (
                "messages_of_a_trade",
                "an uppercase action",
                Box::new(|o| o["data"]["messages"][0]["action"] = json!("Pay-Bond-Invoice")),
            ),
            (
                "messages_of_a_trade",
                "a count that is a number",
                Box::new(|o| o["data"]["keys_read"] = json!(3)),
            ),
            (
                "messages_of_a_trade",
                "a missing truncation flag",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("truncated");
                }),
            ),
            (
                "messages_of_a_trade",
                "a write stage",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
            (
                "messages_of_a_trade",
                "no snapshot",
                Box::new(|o| o["meta"]["snapshot"] = Value::Null),
            ),
            (
                "messages_of_a_trade",
                "an uncertain read",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "more_messages_and_keys_than_were_read",
                "a warning of another command",
                Box::new(|o| o["warnings"][0]["code"] = json!("orders_not_returned")),
            ),
            (
                "order_unknown",
                "a store write error",
                Box::new(|o| o["error"]["code"] = json!("store_unwritable")),
            ),
            (
                "read_timeout",
                "data on an error",
                Box::new(|o| o["data"] = json!({"messages": []})),
            ),
        ];
        let schema = read_json(MESSAGE_LIST_FIXTURE);
        for (case, label, edit) in variants {
            let (_, mut outcome) = message_example(case);
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }

    // --- Identity commands (spec 009) ---

    const IDENTITY_EXAMPLES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/009-identity-json/contracts/examples.json"
    );

    fn identity_examples() -> Vec<(String, i32, Value)> {
        let document = read_json(IDENTITY_EXAMPLES);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn identity_example(name: &str) -> (i32, Value) {
        identity_examples()
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(_, exit, outcome)| (exit, outcome))
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    fn identity_fixture(command: &str) -> Value {
        match command {
            "identity.init" => read_json(IDENTITY_INIT_FIXTURE),
            "identity.show" => read_json(IDENTITY_SHOW_FIXTURE),
            other => panic!("no identity fixture for {other}"),
        }
    }

    /// Every example, built with the owned constructors: the key is the
    /// generator point's (secret key 1), as in the other examples.
    fn identity_cases() -> Vec<(&'static str, Value, i32)> {
        let key = node();
        let init_error = |code, persistence| {
            to_value(Envelope::<IdentityInitData, ()>::error(
                Some(IDENTITY_INIT),
                code,
                Warnings::new(),
                ExecutionMeta::local(persistence),
            ))
            .unwrap()
        };
        let show_error = |code| {
            to_value(Envelope::<IdentityShowData, ()>::error(
                Some(IDENTITY_SHOW),
                code,
                Warnings::new(),
                ExecutionMeta::read(None),
            ))
            .unwrap()
        };
        let show = |index| {
            to_value(Envelope::<_, ()>::ok(
                IDENTITY_SHOW,
                IdentityShowData::new(&key, index),
                Warnings::new(),
                ExecutionMeta::read(None),
            ))
            .unwrap()
        };
        vec![
            (
                "init_created",
                to_value(Envelope::<_, ()>::ok(
                    IDENTITY_INIT,
                    IdentityInitData::new(&key),
                    Warnings::new(),
                    ExecutionMeta::local(Persistence::Applied),
                ))
                .unwrap(),
                0,
            ),
            (
                "init_identity_exists",
                init_error(ErrorCode::IdentityExists, Persistence::NotAttempted),
                1,
            ),
            (
                "init_directory_not_created",
                init_error(ErrorCode::StoreUnwritable, Persistence::NotAttempted),
                1,
            ),
            (
                "init_store_not_completed",
                init_error(ErrorCode::StoreUnwritable, Persistence::Failed),
                1,
            ),
            (
                "init_invalid_arguments",
                init_error(ErrorCode::InvalidArguments, Persistence::NotApplicable),
                2,
            ),
            ("show_new_identity", show(0), 0),
            ("show_after_three_trades", show(3), 0),
            (
                "show_identity_missing",
                show_error(ErrorCode::IdentityMissing),
                1,
            ),
            (
                "show_identity_unreadable",
                show_error(ErrorCode::IdentityUnreadable),
                1,
            ),
            (
                "show_invalid_arguments",
                show_error(ErrorCode::InvalidArguments),
                2,
            ),
        ]
    }

    #[test]
    fn proposed_identity_examples_conform_to_the_checked_in_schemas() {
        let examples = identity_examples();
        assert_eq!(examples.len(), 10);
        for (name, exit, outcome) in examples {
            let schema = identity_fixture(outcome["command"].as_str().unwrap());
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{name}");
            let expected = match (
                outcome["status"].as_str(),
                outcome["error"]["code"].as_str(),
            ) {
                (Some("ok"), _) => 0,
                (_, Some("invalid_arguments")) => 2,
                _ => 1,
            };
            assert_eq!(exit, expected, "{name}");
            assert_eq!(
                outcome["data"].is_null(),
                outcome["status"] != "ok",
                "{name}: data exactly on success"
            );
        }
    }

    #[test]
    fn identity_examples_are_reproduced_by_the_owned_constructors() {
        let cases = identity_cases();
        assert_eq!(cases.len(), identity_examples().len());
        for (name, built, exit) in cases {
            let (example_exit, example) = identity_example(name);
            assert_eq!(built, example, "{name}");
            assert_eq!(exit, example_exit, "{name}");
        }
    }

    #[test]
    fn invalid_identity_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "init_created",
                "an uppercase key",
                Box::new(|o| {
                    let key = o["data"]["identity_pubkey"]
                        .as_str()
                        .unwrap()
                        .to_uppercase();
                    o["data"]["identity_pubkey"] = json!(key);
                }),
            ),
            (
                "init_created",
                "a success that wrote nothing",
                Box::new(|o| o["meta"]["persistence"] = json!("not_attempted")),
            ),
            (
                "init_created",
                "a publication",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
            (
                "init_created",
                "no key",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("identity_pubkey");
                }),
            ),
            (
                "init_identity_exists",
                "a code of show",
                Box::new(|o| o["error"]["code"] = json!("identity_missing")),
            ),
            (
                "init_identity_exists",
                "an uncertain status",
                Box::new(|o| o["status"] = json!("uncertain")),
            ),
            (
                "init_identity_exists",
                "data on an error",
                Box::new(|o| o["data"] = json!({"identity_pubkey": NODE})),
            ),
            (
                "show_after_three_trades",
                "a decimal-string index",
                Box::new(|o| o["data"]["last_trade_index"] = json!("3")),
            ),
            (
                "show_after_three_trades",
                "a negative index",
                Box::new(|o| o["data"]["last_trade_index"] = json!(-1)),
            ),
            (
                "show_after_three_trades",
                "an index past the last address",
                Box::new(|o| o["data"]["last_trade_index"] = json!(2_147_483_648_i64)),
            ),
            (
                "show_after_three_trades",
                "no index",
                Box::new(|o| {
                    o["data"]
                        .as_object_mut()
                        .unwrap()
                        .remove("last_trade_index");
                }),
            ),
            (
                "show_after_three_trades",
                "a local write",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "show_identity_missing",
                "a code of init",
                Box::new(|o| o["error"]["code"] = json!("identity_exists")),
            ),
        ];
        for (case, label, edit) in variants {
            let (_, mut outcome) = identity_example(case);
            let schema = identity_fixture(outcome["command"].as_str().unwrap());
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }

    // --- Order mine, dispute open, order rate (specs 011, 013, 014) ---

    fn examples_of(feature: &str) -> Vec<(String, i32, Value)> {
        let path = format!(
            "{}/specs/{feature}/contracts/examples.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let document = read_json(&path);
        assert_eq!(document["contract_status"], "proposed");
        document["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                (
                    case["name"].as_str().unwrap().to_string(),
                    i32::try_from(case["exit_code"].as_i64().unwrap()).unwrap(),
                    case["stdout"].clone(),
                )
            })
            .collect()
    }

    fn written_write(
        publication: Publication,
        acceptance: DaemonAcceptance,
        persistence: Persistence,
    ) -> ExecutionMeta<()> {
        ExecutionMeta::write(publication, acceptance, persistence)
    }

    fn not_sent() -> ExecutionMeta<()> {
        written_write(
            Publication::NotAttempted,
            DaemonAcceptance::Unknown,
            Persistence::NotAttempted,
        )
    }

    fn mine_cases() -> Vec<(&'static str, Value, i32)> {
        let trade = PublicKey::from_hex(TRADE).unwrap();
        let ok = |orders: Vec<MineOrder>, warnings: Warnings| {
            to_value(Envelope::<_, ()>::ok(
                ORDER_MINE,
                OrderMineData::new(orders),
                warnings,
                ExecutionMeta::read(None),
            ))
            .unwrap()
        };
        let error = |code| {
            to_value(Envelope::<OrderMineData, ()>::error(
                Some(ORDER_MINE),
                code,
                Warnings::new(),
                ExecutionMeta::read(None),
            ))
            .unwrap()
        };
        let row = |n: u128, side, status, key: Option<(&PublicKey, Option<i64>)>| {
            MineOrder::from_row(&id(n).to_string(), side, status, key).unwrap()
        };
        let mut limited = Warnings::new();
        limited.add(WarningCode::TradeKeysLimited);
        vec![
            (
                "orders_newest_first",
                ok(
                    vec![
                        row(5, "sell", "pending", Some((&trade, Some(3)))),
                        row(1, "buy", "waiting-payment", Some((&node(), Some(1)))),
                    ],
                    Warnings::new(),
                ),
                0,
            ),
            (
                "a_key_older_than_the_derived_ones",
                ok(
                    vec![
                        row(2, "sell", "success", Some((&trade, None))),
                        row(3, "sell", "canceled", None),
                    ],
                    limited,
                ),
                0,
            ),
            ("no_orders", ok(Vec::new(), Warnings::new()), 0),
            ("identity_missing", error(ErrorCode::IdentityMissing), 1),
            ("store_unreadable", error(ErrorCode::StoreUnreadable), 1),
            ("invalid_arguments", error(ErrorCode::InvalidArguments), 2),
        ]
    }

    fn dispute_cases() -> Vec<(&'static str, Value, i32)> {
        let trade = PublicKey::from_hex(TRADE).unwrap();
        let data = DisputeOpenData::from_reply(
            &node(),
            id(1),
            (
                &Action::DisputeInitiatedByYou,
                &Some(Payload::Dispute(id(0xd16), None)),
            ),
            &trade,
        )
        .unwrap();
        let ok = |warnings: Warnings, persistence| {
            to_value(Envelope::<_, ()>::ok(
                DISPUTE_OPEN,
                data.clone(),
                warnings,
                written_write(
                    Publication::Published,
                    DaemonAcceptance::Accepted,
                    persistence,
                ),
            ))
            .unwrap()
        };
        let mut stale = Warnings::new();
        stale.add(WarningCode::LocalStatusStale);
        let error = |code| {
            to_value(Envelope::<DisputeOpenData, ()>::error(
                Some(DISPUTE_OPEN),
                code,
                Warnings::new(),
                not_sent(),
            ))
            .unwrap()
        };
        vec![
            (
                "dispute_opened",
                ok(Warnings::new(), Persistence::Applied),
                0,
            ),
            (
                "dispute_opened_status_stale",
                ok(stale, Persistence::Failed),
                0,
            ),
            (
                "node_refused",
                to_value(Envelope::<DisputeOpenData, ()>::refused(
                    Some(DISPUTE_OPEN),
                    "not_allowed_by_status",
                    Warnings::new(),
                    written_write(
                        Publication::Published,
                        DaemonAcceptance::Rejected,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "no_answer_uncertain",
                to_value(Envelope::<DisputeOpenData, ()>::uncertain(
                    DISPUTE_OPEN,
                    ErrorCode::ReplyTimeout,
                    None,
                    Warnings::new(),
                    written_write(
                        Publication::Published,
                        DaemonAcceptance::Unknown,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap(),
                1,
            ),
            ("order_unknown", error(ErrorCode::OrderUnknown), 1),
            ("yes_required", error(ErrorCode::InvalidArguments), 2),
        ]
    }

    fn rate_cases() -> Vec<(&'static str, Value, i32)> {
        let trade = PublicKey::from_hex(TRADE).unwrap();
        let input = RateInput::validate(id(1), 5).unwrap();
        let data = RateData::from_reply(
            &node(),
            &input,
            (&Action::RateReceived, &Some(Payload::RatingUser(5))),
            &trade,
        )
        .unwrap();
        vec![
            (
                "rating_received",
                to_value(Envelope::<_, ()>::ok(
                    ORDER_RATE,
                    data,
                    Warnings::new(),
                    written_write(
                        Publication::Published,
                        DaemonAcceptance::Accepted,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap(),
                0,
            ),
            (
                "node_refused",
                to_value(Envelope::<RateData, ()>::refused(
                    Some(ORDER_RATE),
                    "invalid_order_status",
                    Warnings::new(),
                    written_write(
                        Publication::Published,
                        DaemonAcceptance::Rejected,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "no_answer_uncertain",
                to_value(Envelope::<RateData, ()>::uncertain(
                    ORDER_RATE,
                    ErrorCode::ReplyTimeout,
                    None,
                    Warnings::new(),
                    written_write(
                        Publication::Published,
                        DaemonAcceptance::Unknown,
                        Persistence::NotAttempted,
                    ),
                ))
                .unwrap(),
                1,
            ),
            (
                "rating_out_of_range",
                to_value(Envelope::<RateData, ()>::error(
                    Some(ORDER_RATE),
                    ErrorCode::InvalidArguments,
                    Warnings::new(),
                    not_sent(),
                ))
                .unwrap(),
                2,
            ),
        ]
    }

    type Cases = fn() -> Vec<(&'static str, Value, i32)>;

    const NEW_COMMAND_EXAMPLES: [(&str, &str, Cases); 3] = [
        ("011-order-mine", ORDER_MINE_FIXTURE, mine_cases),
        ("013-dispute-open", DISPUTE_OPEN_FIXTURE, dispute_cases),
        ("014-order-rate", ORDER_RATE_FIXTURE, rate_cases),
    ];

    #[test]
    fn mine_dispute_and_rate_examples_conform_and_are_reproduced() {
        for (feature, fixture, cases) in NEW_COMMAND_EXAMPLES {
            let schema = read_json(fixture);
            let examples = examples_of(feature);
            let built = cases();
            assert_eq!(examples.len(), built.len(), "{feature}");
            for ((name, exit, outcome), (built_name, built_outcome, built_exit)) in
                examples.iter().zip(&built)
            {
                assert_eq!(name, built_name, "{feature}");
                assert_eq!(outcome, built_outcome, "{feature}: {name}");
                assert_eq!(exit, built_exit, "{feature}: {name}");
                assert!(
                    schema_check::conforms(&schema, &schema, outcome),
                    "{feature}: {name}"
                );
                assert_eq!(
                    outcome["data"].is_null(),
                    outcome["status"] != "ok",
                    "{feature}: {name}"
                );
            }
        }
    }

    #[test]
    fn invalid_mine_dispute_and_rate_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, &str, Edit)> = vec![
            (
                "011-order-mine",
                "orders_newest_first",
                "a numeric index",
                Box::new(|o| o["data"]["orders"][0]["trade_index"] = json!(3)),
            ),
            (
                "011-order-mine",
                "orders_newest_first",
                "an unknown status",
                Box::new(|o| o["data"]["orders"][0]["status"] = json!("lost")),
            ),
            (
                "011-order-mine",
                "orders_newest_first",
                "an unknown side",
                Box::new(|o| o["data"]["orders"][0]["side"] = json!("both")),
            ),
            (
                "011-order-mine",
                "orders_newest_first",
                "a local write",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "011-order-mine",
                "identity_missing",
                "a code of another command",
                Box::new(|o| o["error"]["code"] = json!("order_unknown")),
            ),
            (
                "013-dispute-open",
                "dispute_opened",
                "no dispute id",
                Box::new(|o| {
                    o["data"].as_object_mut().unwrap().remove("dispute_id");
                }),
            ),
            (
                "013-dispute-open",
                "dispute_opened",
                "an uppercase dispute id",
                Box::new(|o| {
                    o["data"]["dispute_id"] = json!("00000000-0000-0000-0000-000000000D16")
                }),
            ),
            (
                "013-dispute-open",
                "dispute_opened",
                "a success not published",
                Box::new(|o| o["meta"]["publication"] = json!("not_attempted")),
            ),
            (
                "013-dispute-open",
                "no_answer_uncertain",
                "uncertain with a local write",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "014-order-rate",
                "rating_received",
                "a rating of six",
                Box::new(|o| o["data"]["rating"] = json!(6)),
            ),
            (
                "014-order-rate",
                "rating_received",
                "a rating as text",
                Box::new(|o| o["data"]["rating"] = json!("5")),
            ),
            (
                "014-order-rate",
                "rating_received",
                "a local write",
                Box::new(|o| o["meta"]["persistence"] = json!("applied")),
            ),
            (
                "014-order-rate",
                "node_refused",
                "a warning of another command",
                Box::new(|o| {
                    o["warnings"] = json!([{"code": "local_status_stale",
                 "message": "The node accepted the request but the local order status could not be updated."}])
                }),
            ),
        ];
        for (feature, case, label, edit) in variants {
            let (_, fixture, _) = NEW_COMMAND_EXAMPLES
                .iter()
                .find(|(name, ..)| *name == feature)
                .unwrap();
            let schema = read_json(fixture);
            let (_, _, mut outcome) = examples_of(feature)
                .into_iter()
                .find(|(name, ..)| name == case)
                .unwrap();
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {feature}/{case}"
            );
        }
    }

    // --- Order chat (spec 019) ---

    use mostro_client::outcome::chat::{ChatData, ChatRecord, ChatSender, COMMAND as ORDER_CHAT};

    fn chat_cases() -> Vec<(&'static str, Value, i32)> {
        let record = |byte, from, text: &str| {
            let id = EventId::from_byte_array([byte; 32]);
            ChatRecord::new(&id, from, 1_792_108_800, text, &mut Warnings::new())
        };
        let data = |sent: Option<ChatRecord>| {
            let messages = vec![
                record(0x11, ChatSender::Me, "Fiat sent by bank transfer."),
                record(0x22, ChatSender::Counterparty, "Received, releasing now."),
            ];
            let counterparty = PublicKey::from_hex(TRADE).unwrap();
            ChatData::new(id(1), &counterparty, (messages, false), sent)
        };
        let sent = || Some(record(0x33, ChatSender::Me, "Thanks!"));
        let relays =
            |publication| ExecutionMeta::request(publication, DaemonAcceptance::NotApplicable);
        let ok =
            |data, meta| to_value(Envelope::ok(ORDER_CHAT, data, Warnings::new(), meta)).unwrap();
        let error = |code, meta| {
            let failed =
                Envelope::<ChatData, ()>::error(Some(ORDER_CHAT), code, Warnings::new(), meta);
            to_value(failed).unwrap()
        };
        let uncertain = Envelope::uncertain(
            ORDER_CHAT,
            ErrorCode::RequestRejected,
            Some(data(sent())),
            Warnings::new(),
            relays(Publication::Unknown),
        );
        vec![
            (
                "conversation_read",
                ok(data(None), ExecutionMeta::read(None)),
                0,
            ),
            (
                "message_sent",
                ok(data(sent()), relays(Publication::Published)),
                0,
            ),
            (
                "relays_refused",
                error(
                    ErrorCode::RequestRejected,
                    relays(Publication::NotAttempted),
                ),
                1,
            ),
            ("send_uncertain", to_value(uncertain).unwrap(), 1),
            (
                "counterparty_unknown",
                error(ErrorCode::CounterpartyUnknown, ExecutionMeta::read(None)),
                1,
            ),
            (
                "message_without_yes",
                error(ErrorCode::InvalidArguments, ExecutionMeta::read(None)),
                2,
            ),
        ]
    }

    fn chat_schema() -> Value {
        read_json(ORDER_CHAT_FIXTURE)
    }

    #[test]
    fn chat_examples_conform_and_are_reproduced() {
        let schema = chat_schema();
        let examples = examples_of("019-order-chat");
        let built = chat_cases();
        assert_eq!(examples.len(), built.len());
        for ((name, exit, outcome), (built_name, built_outcome, built_exit)) in
            examples.iter().zip(&built)
        {
            assert_eq!(name, built_name);
            assert_eq!((outcome, exit), (built_outcome, built_exit), "{name}");
            assert!(schema_check::conforms(&schema, &schema, outcome), "{name}");
            let failed = outcome["status"] == "error";
            assert_eq!(outcome["data"].is_null(), failed, "{name}");
        }
    }

    #[test]
    fn invalid_chat_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, &str, Edit)> = vec![
            (
                "conversation_read",
                "a third writer",
                Box::new(|o| o["data"]["messages"][0]["from"] = json!("solver")),
            ),
            (
                "conversation_read",
                "a numeric time",
                Box::new(|o| o["data"]["messages"][0]["created_at_unix"] = json!(1_792_108_800)),
            ),
            (
                "conversation_read",
                "a snapshot",
                Box::new(|o| o["meta"]["snapshot"] = json!({})),
            ),
            (
                "message_sent",
                "acceptance by the node",
                Box::new(|o| o["meta"]["daemon_acceptance"] = json!("accepted")),
            ),
            (
                "relays_refused",
                "a refusal after publishing",
                Box::new(|o| o["meta"]["publication"] = json!("published")),
            ),
            (
                "send_uncertain",
                "uncertain without a send",
                Box::new(|o| o["error"]["code"] = json!("read_timeout")),
            ),
            (
                "message_without_yes",
                "a code of another command",
                Box::new(|o| o["error"]["code"] = json!("node_refused")),
            ),
        ];
        let schema = chat_schema();
        for (case, label, edit) in variants {
            let (_, _, mut outcome) = examples_of("019-order-chat")
                .into_iter()
                .find(|(name, ..)| name == case)
                .unwrap();
            assert!(schema_check::conforms(&schema, &schema, &outcome), "{case}");
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label} on {case}"
            );
        }
    }

    // --- Next steps (spec 021) ---

    /// The proposed examples of spec 021, built with the owned constructors: a
    /// failed payout while the node still retries, so the record asks nothing.
    fn next_step_cases() -> Vec<(&'static str, Value, i32)> {
        let failed = message_record(
            Action::PaymentFailed,
            Some(id(1)),
            Some(Payload::PaymentFailed(PaymentFailedInfo {
                payment_attempts: 2,
                payment_retries_interval: 60,
            })),
            1,
        );
        let data = message_data(vec![failed], false, (3, 0));
        let outcome = to_value(MessageEnvelope::ok(
            MESSAGE_LIST,
            to_value(data).unwrap(),
            Warnings::new(),
            ExecutionMeta::read(Some(read_snapshot(2, 1))),
        ))
        .unwrap();
        vec![
            ("payment_failed_while_the_node_retries", outcome, 0),
            ("messages_of_the_trade_steps", trade_steps(), 0),
        ]
    }

    /// The buyer is asked to send the fiat, the node confirms it was sent
    /// (that asks nothing), and the trade's end asks for a rating.
    fn trade_steps() -> Value {
        let step = |byte: u8, action, payload, minute: u64| {
            let message = Message::new_order(Some(id(1)), None, None, action, payload);
            MessageRecord::from_message(
                &EventId::from_byte_array([byte; 32]),
                1_792_108_800 + minute * 60,
                &message,
                Some(3),
                &PublicKey::from_hex(TRADE).unwrap(),
                &mut Warnings::new(),
            )
        };
        let peer = Peer {
            pubkey: PEER.to_string(),
            reputation: None,
        };
        let records = vec![
            step(
                0x31,
                Action::HoldInvoicePaymentAccepted,
                Some(Payload::Order(pending_order(OrderStatus::Active))),
                1,
            ),
            step(0x32, Action::FiatSentOk, Some(Payload::Peer(peer)), 2),
            step(0x33, Action::Rate, None, 3),
        ];
        let data = message_data(records, false, (3, 0));
        to_value(MessageEnvelope::ok(
            MESSAGE_LIST,
            to_value(data).unwrap(),
            Warnings::new(),
            ExecutionMeta::read(Some(read_snapshot(2, 3))),
        ))
        .unwrap()
    }

    fn next_step_example(name: &str) -> Value {
        examples_of("021-next-steps")
            .into_iter()
            .find(|(candidate, ..)| candidate == name)
            .map(|(.., outcome)| outcome)
            .unwrap_or_else(|| panic!("no example named {name}"))
    }

    #[test]
    fn next_step_examples_conform_to_the_checked_in_schema() {
        let schemas = [
            read_json(MESSAGE_LIST_FIXTURE),
            generated_output_schema(MESSAGE_LIST),
        ];
        for (name, exit, outcome) in examples_of("021-next-steps") {
            assert_eq!(exit, 0, "{name}");
            for schema in &schemas {
                assert!(schema_check::conforms(schema, schema, &outcome), "{name}");
            }
        }
    }

    #[test]
    fn next_step_examples_are_reproduced_by_the_owned_constructors() {
        let examples = examples_of("021-next-steps");
        let built = next_step_cases();
        assert_eq!(examples.len(), built.len());
        for ((name, exit, outcome), (built_name, built_outcome, built_exit)) in
            examples.iter().zip(&built)
        {
            assert_eq!(name, built_name);
            assert_eq!((outcome, exit), (built_outcome, built_exit), "{name}");
        }
    }

    #[test]
    fn invalid_next_step_variants_do_not_conform() {
        type Edit = Box<dyn Fn(&mut Value)>;
        let variants: Vec<(&str, Edit)> = vec![
            (
                "a count that is a number",
                Box::new(|o| o["data"]["messages"][0]["payload"]["payment_attempts"] = json!(2)),
            ),
            (
                "an interval that is not a decimal",
                Box::new(|o| {
                    o["data"]["messages"][0]["payload"]["payment_retries_interval"] = json!("1m")
                }),
            ),
            (
                "a missing count",
                Box::new(|o| {
                    o["data"]["messages"][0]["payload"]
                        .as_object_mut()
                        .unwrap()
                        .remove("payment_attempts");
                }),
            ),
        ];
        let schema = read_json(MESSAGE_LIST_FIXTURE);
        for (label, edit) in variants {
            let mut outcome = next_step_example("payment_failed_while_the_node_retries");
            assert!(schema_check::conforms(&schema, &schema, &outcome));
            edit(&mut outcome);
            assert!(
                !schema_check::conforms(&schema, &schema, &outcome),
                "accepted: {label}"
            );
        }
    }

    // --- Order watch (spec 020) ---

    use mostro_client::outcome::watch::{Cursor, Observed, WatchData, COMMAND as ORDER_WATCH};

    /// The counterparty of the watched trade, not the user's trade key.
    const PEER: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

    /// The proposed examples of `order.watch`, built with the owned
    /// constructors: a node message and a chat line from the counterparty,
    /// with the cursor that the call advanced over them.
    fn watch_cases() -> Vec<(&'static str, Value, i32)> {
        let at = 1_792_108_800;
        let message = message_record(Action::FiatSentOk, Some(id(1)), None, 1);
        let line_id = EventId::from_byte_array([0x22; 32]);
        let line = ChatRecord::new(
            &line_id,
            ChatSender::Counterparty,
            at,
            "Fiat sent by bank transfer.",
            &mut Warnings::new(),
        );
        let observed = [
            Observed::mail(EventId::from_byte_array([7; 32]), at + 60),
            Observed::chat(line_id, at, at),
        ];
        let cursor = Cursor::start(at - 1_800).advance(&observed, at + 120);
        let counterparty = PublicKey::from_hex(PEER).unwrap();
        let records = (vec![message], vec![line]);
        let data = WatchData::new(id(1), Some(&counterparty), records, false, false, &cursor);
        // The next call passes that cursor back and reaches its deadline with
        // nothing newer: empty lists and the given cursor carried forward.
        let given = Cursor::parse(&cursor.to_string()).unwrap();
        let empty = WatchData::new(
            id(1),
            Some(&counterparty),
            (vec![], vec![]),
            false,
            false,
            &given,
        );
        let ok = |data| {
            to_value(Envelope::<WatchData, ()>::ok(
                ORDER_WATCH,
                data,
                Warnings::new(),
                ExecutionMeta::read(None),
            ))
            .unwrap()
        };
        vec![
            ("new_records", ok(data), 0),
            ("timeout_empty", ok(empty), 0),
        ]
    }

    #[test]
    fn watch_examples_conform_and_are_reproduced() {
        let discovered = read_json(DISCOVERY_FIXTURE)["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["name"] == ORDER_WATCH)
            .map(|command| command["output_schema"].clone())
            .unwrap();
        let fixture = read_json(WATCH_FIXTURE);
        let examples = examples_of("020-order-watch");
        let built = watch_cases();
        assert_eq!(examples.len(), built.len());
        for ((name, exit, outcome), (built_name, built_outcome, built_exit)) in
            examples.iter().zip(&built)
        {
            assert_eq!(name, built_name);
            assert_eq!((outcome, exit), (built_outcome, built_exit), "{name}");
            for schema in [&discovered, &fixture] {
                assert!(schema_check::conforms(schema, schema, outcome), "{name}");
            }
        }
    }
}

// --- Source boundaries (T050) --------------------------------------------
//
// Static checks over `src/`. They apply the constitution's size and import
// rules to production code, freeze the legacy exceptions instead of excusing
// them, and keep network access with its owner. They do not claim that the
// legacy code is migrated.

mod source_boundaries {
    use super::{read_dir, read_to_string, BTreeMap, Path};

    const SRC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    const LIMIT: usize = 400;

    /// Legacy files over the limit, frozen at their measured production size.
    /// A file leaves this list when its migration brings it under the limit.
    const LEGACY_OVERSIZED: [(&str, usize); 4] = [
        ("cli/take_dispute.rs", 403),
        ("db.rs", 629),
        ("parser/dms.rs", 1189),
        ("util/messaging.rs", 556),
    ];

    /// Function-local production imports that remain as legacy debt.
    const LEGACY_LOCAL_IMPORTS: [(&str, &str); 3] = [
        ("cli/send_admin_dm_attach.rs", "use reqwest::StatusCode;"),
        ("util/misc.rs", "use std::os::unix::fs::DirBuilderExt;"),
        ("util/misc.rs", "use std::os::unix::fs::PermissionsExt;"),
    ];

    /// Calls that publish, fetch, subscribe or connect, with the files that may
    /// contain them and their frozen counts. `msg` owns public reads; the other
    /// entries are legacy transport that a later migration moves behind it.
    const TRANSPORT_CALLS: [(&str, &[(&str, usize)]); 4] = [
        (
            ".send_event(",
            &[
                ("cli/dm_to_user.rs", 1),
                ("cli/send_admin_dm_attach.rs", 1),
                ("msg/send_msg.rs", 1),
                ("util/messaging.rs", 2),
            ],
        ),
        (
            ".fetch_events(",
            &[
                ("cli/get_dm_user.rs", 1),
                ("msg/node_info.rs", 1),
                ("util/events.rs", 8),
            ],
        ),
        (
            ".subscribe(",
            &[
                ("msg/exchange.rs", 1),
                ("msg/receive_msg.rs", 1),
                ("msg/watch.rs", 1),
                ("util/messaging.rs", 1),
            ],
        ),
        (".add_relay(", &[("msg/mod.rs", 1), ("util/net.rs", 1)]),
    ];

    pub struct Source {
        pub path: String,
        lines: Vec<String>,
        production: usize,
        /// Declared only under `#[cfg(test)]`; see [`is_test_only`].
        test_only: bool,
    }

    impl Source {
        /// Production lines, comments and blanks included.
        fn production(&self) -> &[String] {
            &self.lines[..self.production]
        }

        /// Code lines of the production region, without line comments.
        fn code(&self) -> impl Iterator<Item = (usize, &str)> {
            self.production()
                .iter()
                .enumerate()
                .map(|(index, line)| (index + 1, line.as_str()))
                .filter(|(_, line)| !line.trim_start().starts_with("//"))
        }
    }

    pub fn sources() -> Vec<Source> {
        fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }
        let mut paths = Vec::new();
        walk(Path::new(SRC), &mut paths);
        let texts: BTreeMap<String, Vec<String>> = paths
            .into_iter()
            .map(|path| {
                let text = read_to_string(&path).unwrap();
                let relative = path
                    .strip_prefix(SRC)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                (relative, text.lines().map(str::to_string).collect())
            })
            .collect();
        texts
            .iter()
            .map(|(path, lines)| Source {
                path: path.clone(),
                lines: lines.clone(),
                production: production_len(path, lines),
                test_only: is_test_only(path, &texts),
            })
            .collect()
    }

    /// Whether `path` is a test-only file: its parent module declares it with
    /// `#[cfg(test)]` directly above `mod name;`, so it is never compiled into
    /// the binary. The constitution excludes such files from the size limit;
    /// the import, transport and no-I/O checks still read every line of them.
    fn is_test_only(path: &str, texts: &BTreeMap<String, Vec<String>>) -> bool {
        let Some((dir, name)) = path
            .strip_suffix(".rs")
            .and_then(|stem| stem.rsplit_once('/'))
        else {
            return false;
        };
        let declaration = format!("mod {name};");
        [format!("{dir}.rs"), format!("{dir}/mod.rs")]
            .iter()
            .filter_map(|parent| texts.get(parent))
            .any(|lines| {
                lines
                    .windows(2)
                    .any(|pair| pair[0].trim() == "#[cfg(test)]" && pair[1].trim() == declaration)
            })
    }

    /// Whether `line` starts a `mod name {` item.
    fn is_inline_mod(line: &str) -> bool {
        let line = line.trim_start();
        let line = line.strip_prefix("pub ").unwrap_or(line);
        line.strip_prefix("mod ")
            .is_some_and(|rest| rest.trim_end().ends_with('{'))
    }

    /// Net brace depth change of `line`, ignoring comments, strings and chars.
    fn brace_delta(line: &str) -> i64 {
        let chars: Vec<char> = line.chars().collect();
        let (mut delta, mut i) = (0i64, 0);
        while i < chars.len() {
            match chars[i] {
                '/' if chars.get(i + 1) == Some(&'/') => break,
                '"' => {
                    i += 1;
                    while i < chars.len() && chars[i] != '"' {
                        i += if chars[i] == '\\' { 2 } else { 1 };
                    }
                }
                '\'' => {
                    // A char literal ('{', '\n'); a lifetime has no closing quote.
                    let close = if chars.get(i + 1) == Some(&'\\') {
                        i + 3
                    } else {
                        i + 2
                    };
                    if chars.get(close) == Some(&'\'') {
                        i = close;
                    }
                }
                '{' => delta += 1,
                '}' => delta -= 1,
                _ => {}
            }
            i += 1;
        }
        delta
    }

    /// Production line count: lines before a trailing `#[cfg(test)] mod`,
    /// minus two-line `#[cfg(test)]` imports. The test module must close on
    /// the last nonblank line, so no production code can hide behind it.
    fn production_len(path: &str, lines: &[String]) -> usize {
        let mut cfg_imports = 0;
        let mut tests = None;
        for (index, line) in lines.iter().enumerate() {
            if line.trim() != "#[cfg(test)]" {
                continue;
            }
            let next = lines[index + 1..]
                .iter()
                .find(|line| !line.trim_start().starts_with("#["));
            match next {
                Some(next) if is_inline_mod(next) => {
                    tests = Some(index);
                    break;
                }
                Some(next) if next.trim_start().starts_with("use ") => cfg_imports += 2,
                _ => {}
            }
        }
        let Some(start) = tests else {
            return lines.len() - cfg_imports;
        };
        let last = lines
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .unwrap();
        let (mut depth, mut opened) = (0, false);
        for (offset, line) in lines[start..].iter().enumerate() {
            depth += brace_delta(line);
            opened |= depth > 0;
            let position = start + offset;
            assert!(
                !opened || depth > 0 || position == last,
                "{path}: the test module closes at line {} but the file ends at line {}",
                position + 1,
                last + 1
            );
        }
        assert_eq!(depth, 0, "{path}: unbalanced braces in the test module");
        start - cfg_imports
    }

    #[test]
    fn counter_applies_the_documented_exclusions() {
        let text = |body: &str| body.lines().map(str::to_string).collect::<Vec<_>>();
        let plain = text("fn a() {}\n// c\n\nfn b() {}");
        assert_eq!(
            production_len("plain", &plain),
            4,
            "comments and blanks count"
        );
        let with_tests = text(
            "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = '{'; let _ = \"}\"; }\n}\n",
        );
        assert_eq!(production_len("tests", &with_tests), 1);
        let with_import = text("use a;\n#[cfg(test)]\nuse b;\nfn a() {}\n");
        assert_eq!(production_len("import", &with_import), 2);

        let tree: BTreeMap<String, Vec<String>> = [
            ("a.rs", "#[cfg(test)]\nmod tests;\nmod b;\n"),
            ("a/b.rs", "fn b() {}\n"),
            ("a/tests.rs", "fn t() {}\n"),
            ("c/mod.rs", "#[cfg(test)]\nmod tests;\n"),
            ("c/tests.rs", "fn t() {}\n"),
            ("d.rs", "#[cfg(test)]\nuse x;\nmod tests;\n"),
            ("d/tests.rs", "fn t() {}\n"),
        ]
        .into_iter()
        .map(|(path, body)| (path.to_string(), text(body)))
        .collect();
        assert!(
            is_test_only("a/tests.rs", &tree),
            "declared under cfg(test)"
        );
        assert!(is_test_only("c/tests.rs", &tree), "declared from a mod.rs");
        assert!(!is_test_only("a/b.rs", &tree), "an unguarded module");
        assert!(!is_test_only("a.rs", &tree), "a top-level file");
        assert!(
            !is_test_only("d/tests.rs", &tree),
            "cfg(test) guards the import"
        );
    }

    #[test]
    #[should_panic(expected = "closes at line")]
    fn production_code_after_a_test_module_is_rejected() {
        let lines: Vec<String> = "#[cfg(test)]\nmod tests {\n}\nfn hidden() {}\n"
            .lines()
            .map(str::to_string)
            .collect();
        production_len("hidden", &lines);
    }

    #[test]
    fn production_files_respect_the_size_limit_and_the_frozen_exceptions() {
        let frozen: BTreeMap<_, _> = LEGACY_OVERSIZED.into_iter().collect();
        let mut seen = 0;
        for source in sources().into_iter().filter(|source| !source.test_only) {
            let size = source.production().len();
            match frozen.get(source.path.as_str()) {
                Some(&cap) => {
                    seen += 1;
                    assert!(
                        size > LIMIT,
                        "{}: now {size}; remove its exception",
                        source.path
                    );
                    assert!(
                        size <= cap,
                        "{}: legacy production grew to {size} (frozen at {cap})",
                        source.path
                    );
                }
                None => assert!(
                    size <= LIMIT,
                    "{}: {size} production lines exceed {LIMIT}; split it",
                    source.path
                ),
            }
        }
        assert_eq!(
            seen,
            LEGACY_OVERSIZED.len(),
            "an excepted file no longer exists"
        );
    }

    #[test]
    fn production_imports_are_at_the_top_except_frozen_legacy() {
        let allowed: Vec<_> = LEGACY_LOCAL_IMPORTS.into_iter().collect();
        let item = |line: &str| {
            let line = line.strip_prefix("pub ").unwrap_or(line);
            let line = line.strip_prefix("async ").unwrap_or(line);
            [
                "fn ", "struct ", "enum ", "impl", "trait ", "const ", "static ", "type ",
            ]
            .iter()
            .any(|keyword| line.starts_with(keyword))
        };
        for source in sources() {
            let mut after_item = false;
            for (number, line) in source.code() {
                let trimmed = line.trim();
                let top_level = !line.starts_with(char::is_whitespace);
                if top_level && item(line) {
                    after_item = true;
                }
                if !trimmed.starts_with("use ") && !trimmed.starts_with("pub use ") {
                    continue;
                }
                let local = !top_level;
                assert!(
                    !(local || after_item) || allowed.contains(&(source.path.as_str(), trimmed)),
                    "{}:{number}: import is not at the top of the file: {trimmed}",
                    source.path
                );
            }
        }
    }

    #[test]
    fn network_calls_stay_with_their_owner_and_do_not_grow() {
        let mut found: BTreeMap<(&str, String), usize> = BTreeMap::new();
        for source in sources() {
            for (_, line) in source.code() {
                for (call, _) in TRANSPORT_CALLS {
                    if line.contains(call) {
                        *found.entry((call, source.path.clone())).or_default() += 1;
                    }
                }
            }
        }
        for ((call, path), count) in &found {
            let allowed = TRANSPORT_CALLS
                .iter()
                .find(|(name, _)| name == call)
                .and_then(|(_, files)| files.iter().find(|(file, _)| file == path))
                .map(|(_, cap)| *cap);
            assert!(
                allowed.is_some_and(|cap| *count <= cap),
                "{path}: `{call}` x{count} is outside its owner or beyond the frozen count; \
                 publish and fetch through msg, not directly"
            );
        }
        for (call, files) in TRANSPORT_CALLS {
            for (file, cap) in files {
                assert_eq!(
                    found.get(&(call, file.to_string())).copied().unwrap_or(0),
                    *cap,
                    "{file}: `{call}` count changed; update the frozen list when it migrates"
                );
            }
        }
    }

    /// Modules that must stay free of I/O, clients and storage, and what is
    /// forbidden in each.
    const NO_IO: [(&str, &[&str]); 7] = [
        (
            "parse_human/",
            &[
                "println!",
                "print!(",
                "eprint",
                "process::exit",
                "std::fs",
                "std::env",
                "sqlx",
                "tokio::",
                "reqwest",
                "Client",
                "crate::db",
                "crate::msg",
                "crate::util::messaging",
            ],
        ),
        (
            "parse_machine.rs",
            &[
                "println!",
                "print!(",
                "eprint",
                "process::exit",
                "std::fs",
                "std::env",
                "sqlx",
                "tokio::",
                "reqwest",
                "Client",
                "crate::db",
                "crate::msg",
            ],
        ),
        (
            "commands/",
            &[
                "println!",
                "print!(",
                "eprint",
                "process::exit",
                "std::fs",
                "std::env",
                "sqlx",
                "reqwest",
                "Client",
                "crate::db",
                "send_event",
                ".subscribe(",
                "fetch_events",
                "std::io",
            ],
        ),
        (
            "outcome",
            &[
                "println!",
                "print!(",
                "eprint",
                "process::exit",
                "std::fs",
                "std::env",
                "sqlx",
                "tokio::",
                "reqwest",
                "Client",
                "crate::db",
                "crate::msg",
                "crate::util",
            ],
        ),
        (
            "cli/schema.rs",
            &[
                "println!",
                "print!(",
                "eprint",
                "process::exit",
                "std::fs",
                "std::env",
                "sqlx",
                "tokio::",
                "reqwest",
                "Client",
                "crate::db",
                "crate::msg",
                "crate::util",
            ],
        ),
        (
            "cli/registry.rs",
            &[
                "println!",
                "print!(",
                "eprint",
                "process::exit",
                "std::fs",
                "std::env",
                "sqlx",
                "tokio::",
                "reqwest",
                "Client",
                "crate::db",
                "crate::msg",
                "crate::util",
            ],
        ),
        (
            "cli/input_schema.rs",
            &[
                "println!",
                "print!(",
                "eprint",
                "process::exit",
                "std::fs",
                "std::env",
                "sqlx",
                "tokio::",
                "reqwest",
                "Client",
                "crate::db",
                "crate::msg",
                "crate::util",
            ],
        ),
    ];

    #[test]
    fn renderers_operations_and_outcomes_perform_no_io() {
        let mut checked = 0;
        for source in sources() {
            for (prefix, forbidden) in NO_IO {
                if !source.path.starts_with(prefix) {
                    continue;
                }
                checked += 1;
                for (number, line) in source.code() {
                    for word in forbidden {
                        assert!(
                            !line.contains(word),
                            "{}:{number}: `{word}` is not allowed here: {}",
                            source.path,
                            line.trim()
                        );
                    }
                }
            }
        }
        assert!(
            checked >= 12,
            "only {checked} files matched the boundary list"
        );
    }

    /// Files that may reserve a trade index: the operations that publish a
    /// request carrying a fresh trade key.
    const RESERVERS: [&str; 3] = [
        "commands/order/new.rs",
        "commands/order/step.rs",
        "commands/order/take.rs",
    ];

    #[test]
    fn a_published_trade_index_is_always_reserved_never_peeked() {
        // An operation that published a peeked index could reuse a key after a
        // crash or share one with a concurrent run (spec 008, slice 7).
        let mut reservers = Vec::new();
        for source in sources() {
            for (number, line) in source.code() {
                if source.path.starts_with("commands/") {
                    assert!(
                        !line.contains(".next_trade("),
                        "{}:{number}: an operation peeks the trade index: {}",
                        source.path,
                        line.trim()
                    );
                }
                if line.contains("reserve_trade(") && !source.path.starts_with("store") {
                    reservers.push(source.path.clone());
                }
            }
        }
        reservers.sort();
        reservers.dedup();
        assert_eq!(reservers, RESERVERS);
    }

    #[test]
    fn the_public_read_client_is_not_exported_from_msg() {
        let sources = sources();
        let module = sources.iter().find(|s| s.path == "msg/mod.rs").unwrap();
        for (number, line) in module.code() {
            let exported = line.trim_start().starts_with("pub ") && !line.contains("pub(");
            assert!(
                !(exported
                    && (line.contains("Session")
                        || line.contains("Client")
                        || line.contains("client"))),
                "msg/mod.rs:{number}: a public item exposes the client: {}",
                line.trim()
            );
        }
        // The privacy itself is proved by the compile_fail doctests in that file.
        assert!(module
            .lines
            .iter()
            .any(|l| l.contains("compile_fail,E0603")));
    }
}

// --- CI wiring (T051) ----------------------------------------------------

mod ci_wiring {
    use super::read_to_string;

    fn workflow(name: &str) -> String {
        let path = format!("{}/.github/workflows/{name}", env!("CARGO_MANIFEST_DIR"));
        // A Windows checkout may convert line endings; the parsing below uses `\n`.
        read_to_string(&path)
            .unwrap_or_else(|error| panic!("{path}: {error}"))
            .replace("\r\n", "\n")
    }

    /// The text of one top-level job, up to the next job key or job comment.
    fn job<'a>(text: &'a str, id: &str) -> &'a str {
        let marker = format!("\n  {id}:\n");
        let start = text
            .find(&marker)
            .unwrap_or_else(|| panic!("job {id} is missing"))
            + 1;
        let mut end = text.len();
        let mut offset = start;
        for line in text[start..].split_inclusive('\n') {
            let sibling = line.starts_with("  ") && !line.starts_with("   ");
            if offset > start && sibling {
                end = offset;
                break;
            }
            offset += line.len();
        }
        &text[start..end]
    }

    #[test]
    fn ci_runs_the_drift_boundary_and_windows_checks() {
        let ci = workflow("ci.yml");
        let test = job(&ci, "test");
        assert!(test.contains("cargo test --locked --test integration_tests schema_fixtures"));
        assert!(test.contains("cargo test --locked --test integration_tests source_boundaries"));
        assert!(test.contains("cargo test --locked --doc"));
        let windows = job(&ci, "pilot-windows");
        assert!(windows.contains("runs-on: windows-latest"));
        assert!(windows.contains("cargo test --locked"));
        assert!(windows.contains("contents: read"));
        assert!(
            !windows.contains("secrets."),
            "the Windows job must hold no secrets"
        );
        assert!(!windows.contains("sh -c") && !windows.contains("chmod"));
    }

    #[test]
    fn the_release_workflow_gates_publishing_on_the_drift_checks() {
        let release = workflow("rust.yml");
        let test = job(&release, "test");
        assert!(test.contains("cargo test --locked --test integration_tests schema_fixtures"));
        assert!(job(&release, "publish").contains("needs: [test, build]"));
    }

    #[test]
    fn releases_and_ci_build_the_macos_binaries() {
        let path = format!("{}/archs", env!("CARGO_MANIFEST_DIR"));
        let archs = read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
        for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
            assert!(
                archs.lines().any(|line| line.trim() == target),
                "archs lacks {target}"
            );
        }
        let release = workflow("rust.yml");
        assert!(job(&release, "matrix").contains("macos-latest"));
        let build = job(&release, "build");
        assert!(build.contains("runs-on: ${{ matrix.os }}"));
        assert!(build.contains("rustup target add ${{ matrix.target }}"));
        let ci = workflow("ci.yml");
        let macos = job(&ci, "macos");
        assert!(macos.contains("runs-on: macos-latest"));
        assert!(macos.contains("--target aarch64-apple-darwin"));
        assert!(macos.contains("--target x86_64-apple-darwin"));
        // The job runs third-party build scripts: they must not reach the checkout token.
        assert!(macos.contains("persist-credentials: false"));
    }

    #[test]
    fn existing_required_checks_are_kept() {
        let ci = workflow("ci.yml");
        for id in ["fmt", "clippy", "test", "build", "msrv"] {
            assert!(ci.contains(&format!("\n  {id}:\n")), "job {id} was removed");
        }
        assert!(ci.contains("cargo clippy --all-targets --all-features -- -D warnings"));
        assert!(ci.contains("cargo +1.94.0 build --locked --all-features"));
    }

    #[test]
    fn workflows_never_rewrite_or_commit_the_schema_files() {
        for name in ["ci.yml", "rust.yml"] {
            for line in workflow(name).lines() {
                if line.trim_start().starts_with('#') {
                    continue;
                }
                assert!(!line.contains("docs/schemas"), "{name}: {line}");
                for write in ["git add", "git commit", "git push", "cargo run"] {
                    assert!(!line.contains(write), "{name}: {line}");
                }
            }
        }
    }
}
