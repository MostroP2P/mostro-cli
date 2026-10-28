use std::fs::read_to_string;

use chrono::DateTime;
use mostro_client::cli::{offline_discovery, Context};
use mostro_client::outcome::order::ListOrdersInput;
use mostro_client::outcome::{Envelope, ErrorCode, ExecutionMeta, Warnings};
use mostro_client::util::events::create_seven_days_filter_at;
use mostro_client::util::{create_filter, select_reply_event, ExpectedReply, ListKind};
use mostro_core::order::Status;
use mostro_core::prelude::{
    wrap_message_with, Action, Message, MessageKind, Payload, Transport, UserInfo, WrapOptions,
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

const DISCOVERY_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schemas/discovery-v1.json"
);
const LISTING_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schemas/order-list-v1.json"
);
const EXAMPLES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/specs/001-order-list-json/contracts/examples.json"
);

/// How to refresh the fixtures after an intended change. The tests only read
/// them and never rewrite a fixture.
const REGENERATE: &str = "regenerate with: cargo run -q -- schema > docs/schemas/discovery-v1.json; \
cargo run -q -- --output json schema | jq '.data.commands[] | select(.name==\"order.list\") | .output_schema' \
> docs/schemas/order-list-v1.json";

fn read_json(path: &str) -> Value {
    let text = read_to_string(path).unwrap_or_else(|error| panic!("{path}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn generated_discovery() -> Value {
    to_value(offline_discovery()).unwrap()
}

fn generated_listing_schema() -> Value {
    generated_discovery()["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "order.list")
        .map(|command| command["output_schema"].clone())
        .expect("order.list is described")
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
    assert_eq!(machine, ["order.list", "schema"]);
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
    for fixture in [DISCOVERY_FIXTURE, LISTING_FIXTURE] {
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
