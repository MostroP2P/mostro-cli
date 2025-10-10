use mostro_client::parser::dms::{parse_dm_events, print_direct_messages};
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

#[tokio::test]
async fn parse_dm_empty() {
    let keys = Keys::generate();
    let events = Events::new(&Filter::new());
    let out = parse_dm_events(events, &keys, None).await;
    assert!(out.is_empty());
}

#[tokio::test]
async fn print_dms_empty() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let msgs: Vec<(Message, u64, PublicKey)> = Vec::new();
    let res = print_direct_messages(&msgs, &pool).await;
    assert!(res.is_ok());
}
