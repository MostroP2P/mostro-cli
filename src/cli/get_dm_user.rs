use crate::cli::Context;
use crate::db::Order;
use crate::util::{fetch_events_list, Event, ListKind};
use anyhow::Result;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::Table;
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
        println!("No trade keys found in orders");
        return Ok(());
    }

    // Print the number of trade keys
    println!(
        "Searching for DMs in {} trade keys...",
        trade_keys_hex.len()
    );

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
        println!("You don't have any direct messages in your trade keys");
        return Ok(());
    }
    // Extract the direct messages
    for event in direct_messages {
        if let Event::MessageTuple(tuple) = event {
            dm_events.push(*tuple);
        }
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(comfy_table::ContentArrangement::Dynamic)
        .set_header(vec!["Time", "From", "Message"]);

    for (message, created_at, sender_pubkey) in dm_events.iter() {
        let datetime = chrono::DateTime::from_timestamp(*created_at as i64, 0);
        let formatted_date = match datetime {
            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "Invalid timestamp".to_string(),
        };

        let inner = message.get_inner_message_kind();
        let message_str = match &inner.payload {
            Some(Payload::TextMessage(text)) => text.clone(),
            _ => format!("{:?}", message),
        };

        let sender_hex = sender_pubkey.to_hex();

        table.add_row(vec![&formatted_date, &sender_hex, &message_str]);
    }

    println!("{table}");
    println!();
    Ok(())
}
