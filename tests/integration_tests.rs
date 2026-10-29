use std::collections::BTreeMap;
use std::fs::{read_dir, read_to_string};
use std::path::Path;

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

mod schema_fixtures {
    use super::{
        json, offline_discovery, read_to_string, schema_check, to_value, Envelope, ErrorCode,
        ExecutionMeta, Value, Warnings,
    };

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
        ("db.rs", 704),
        ("parser/dms.rs", 1210),
        ("util/messaging.rs", 556),
    ];

    /// Function-local production imports that remain as legacy debt.
    const LEGACY_LOCAL_IMPORTS: [(&str, &str); 4] = [
        ("cli/send_admin_dm_attach.rs", "use reqwest::StatusCode;"),
        ("db.rs", "use std::os::unix::fs::OpenOptionsExt;"),
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
                ("util/messaging.rs", 2),
            ],
        ),
        (
            ".fetch_events(",
            &[
                ("cli/get_dm_user.rs", 1),
                ("cli/send_msg.rs", 1),
                ("util/events.rs", 8),
            ],
        ),
        (
            ".subscribe(",
            &[("msg/receive_msg.rs", 1), ("util/messaging.rs", 1)],
        ),
        (".add_relay(", &[("msg/mod.rs", 1), ("util/net.rs", 1)]),
    ];

    pub struct Source {
        pub path: String,
        lines: Vec<String>,
        production: usize,
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
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                let text = read_to_string(&path).unwrap();
                let lines: Vec<String> = text.lines().map(str::to_string).collect();
                let relative = path
                    .strip_prefix(SRC)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let production = production_len(&relative, &lines);
                Source {
                    path: relative,
                    lines,
                    production,
                }
            })
            .collect()
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
        for source in sources() {
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
    const NO_IO: [(&str, &[&str]); 6] = [
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
