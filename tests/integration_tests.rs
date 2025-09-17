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
        context_keys,
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
    assert!(!ctx.context_keys.public_key().to_hex().is_empty());
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
