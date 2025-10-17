use crate::cli::Context;
use crate::db::Order;
use crate::parser::dms::print_direct_messages;
use crate::util::{fetch_events_list, Event, ListKind};
use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

pub async fn execute_get_dm_user(since: &i64, ctx: &Context) -> Result<()> {
    // Get all trade keys from orders
    let mut trade_keys_hex = Order::get_all_trade_keys(&ctx.pool).await?;

    // Include admin pubkey so we also fetch messages sent TO admin
    let admin_pubkey_hex = ctx.mostro_pubkey.to_hex();
    if !trade_keys_hex.iter().any(|k| k == &admin_pubkey_hex) {
        trade_keys_hex.push(admin_pubkey_hex);
    }
    // De-duplicate any repeated keys coming from DB/admin
    trade_keys_hex.sort();
    trade_keys_hex.dedup();

    // Check if the trade keys are empty
    if trade_keys_hex.is_empty() {
        println!("📭 No trade keys found in orders");
        return Ok(());
    }

    println!("📨 Fetch User Direct Messages");
    println!("═══════════════════════════════════════");
    println!(
        "🔍 Searching for DMs in {} trade keys...",
        trade_keys_hex.len()
    );
    println!("⏰ Since: {} minutes ago", since);
    println!("💡 Fetching direct messages...");
    println!();

    let direct_messages = fetch_events_list(
        ListKind::DirectMessagesUser,
        None,
        None,
        None,
        ctx,
        Some(since),
    )
    .await?;

    // Extract (Message, u64) tuples from Event::MessageTuple variants
    let mut dm_events: Vec<(Message, u64, PublicKey)> = Vec::new();
    // Check if the direct messages are empty
    if direct_messages.is_empty() {
        println!("📭 You don't have any direct messages in your trade keys");
        return Ok(());
    }
    // Extract the direct messages
    for event in direct_messages {
        if let Event::MessageTuple(tuple) = event {
            dm_events.push(*tuple);
        }
    }

    print_direct_messages(&dm_events, &ctx.pool, Some(ctx.mostro_pubkey)).await?;
    Ok(())
}
