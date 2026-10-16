use chrono::DateTime;
use mostro_client::cli::list_orders::parse_status_filter;
use mostro_client::cli::Context;
use mostro_client::util::events::create_seven_days_filter_at;
use mostro_client::util::{create_filter, ListKind};
use mostro_core::order::Status;
use nostr_sdk::prelude::*;
use rstest::rstest;
use serde_json::{json, to_value};
use sqlx::SqlitePool;

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
    assert_eq!(parse_status_filter(input).unwrap(), expected);
}

#[rstest]
#[case("")]
#[case(" pending")]
#[case("pending ")]
#[case("unknown")]
fn listing_status_rejects_invalid_or_padded_values(#[case] input: &str) {
    assert!(parse_status_filter(Some(input)).is_err());
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
