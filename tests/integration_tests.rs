use chrono::DateTime;
use mostro_client::cli::list_orders::parse_status_filter;
use mostro_client::cli::Context;
use mostro_client::util::events::create_seven_days_filter_at;
use mostro_client::util::{create_filter, select_reply_event, ExpectedReply, ListKind};
use mostro_core::order::Status;
use mostro_core::prelude::{
    wrap_message_with, Action, Message, MessageKind, Payload, Transport, UserInfo, WrapOptions,
};
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
