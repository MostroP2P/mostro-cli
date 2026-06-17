use mostro_client::cli::Context;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;

// Helper to create a test context for integration tests
async fn create_test_context() -> anyhow::Result<Context> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;

    // Generate test keys
    let identity_keys = Keys::generate();
    let trade_keys = Keys::generate();
    let context_keys = Keys::generate();

    // Create a test client
    let client = Client::new(identity_keys.clone());

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

    let filter = mostro_client::util::create_filter(
        mostro_client::util::ListKind::Orders,
        ctx.mostro_pubkey,
        None,
        ctx.mostro_pubkey,
    )
    .unwrap();

    assert!(filter.kinds.is_some());
    assert!(filter.authors.is_some());
    assert!(filter
        .authors
        .as_ref()
        .unwrap()
        .contains(&ctx.mostro_pubkey));
}

// Phase 2: `create_filter` for the `DirectMessages*` kinds is transport-aware.
// On v2 (nip44) it must select kind 14 AND pin `author = mostro_pubkey` (kind 14
// is shared with NIP-17 peer chat, so the author pin is what disambiguates the
// Mostro reply); on the default gift-wrap it stays kind 1059 with no author pin
// (the outer event is signed by a throwaway key). The transport is read from the
// `TRANSPORT` env var — no other test in this binary reads it, so set/remove here
// is deterministic.
#[test]
fn direct_messages_filter_is_transport_aware() {
    use mostro_client::util::{create_filter, ListKind};

    let trade = Keys::generate().public_key();
    let mostro = Keys::generate().public_key();

    // v2: kind 14 + author pinned to Mostro.
    std::env::set_var("TRANSPORT", "nip44");
    let v2 = create_filter(ListKind::DirectMessagesUser, trade, None, mostro).unwrap();
    std::env::remove_var("TRANSPORT");
    assert!(v2
        .kinds
        .as_ref()
        .unwrap()
        .contains(&Kind::PrivateDirectMessage));
    assert!(
        v2.authors.as_ref().unwrap().contains(&mostro),
        "v2 DM filter must pin author = mostro_pubkey"
    );

    // Default (gift-wrap): kind 1059, no author pin.
    let v1 = create_filter(ListKind::DirectMessagesUser, trade, None, mostro).unwrap();
    assert!(v1.kinds.as_ref().unwrap().contains(&Kind::GiftWrap));
    assert!(v1.authors.is_none(), "v1 DM filter must not pin an author");
}
