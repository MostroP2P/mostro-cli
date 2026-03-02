use crate::cli::Context;
use crate::db::Order;
use crate::parser::common::{
    print_info_line, print_key_value, print_no_data_message, print_section_header,
};
use crate::util::messaging::{derive_shared_key_bytes, fetch_gift_wraps_for_shared_key};
use anyhow::Result;
use nostr_sdk::prelude::*;
use uuid::Uuid;

/// Fetch user-to-user chat messages over a shared conversation key.
///
/// CLI parameters:
/// - `pubkey`: counterparty pubkey
/// - `order_id`: order used to look up the trade keys
/// - `since`: minutes back in time to include
pub async fn execute_get_dm_user(
    pubkey: PublicKey,
    order_id: Uuid,
    since: &i64,
    ctx: &Context,
) -> Result<()> {
    print_section_header("📨 Fetch User Direct Messages");
    print_key_value("👥", "Counterparty", &pubkey.to_string());
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_key_value("⏰", "Since", &format!("{} minutes ago", since));
    print_info_line("💡", "Fetching shared-key chat messages...");
    println!();

    // 1. Get the order and its trade keys
    let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load order {order_id}: {e}"))?;

    let trade_keys_str = order
        .trade_keys
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Missing trade keys for order {order_id}"))?;
    let trade_keys =
        Keys::parse(&trade_keys_str).map_err(|e| anyhow::anyhow!("Invalid trade keys: {e}"))?;

    // 2. Derive the shared conversation key (trade private key + counterparty pubkey)
    let shared_key_bytes = derive_shared_key_bytes(&trade_keys, &pubkey).map_err(|e| {
        log::warn!(
            "get_dm_user: could not derive shared key (trade + counterparty): {}",
            e
        );
        anyhow::anyhow!("Could not derive shared key for chat with counterparty")
    })?;

    let shared_keys = SecretKey::from_slice(&shared_key_bytes)
        .map(Keys::new)
        .map_err(|e| anyhow::anyhow!("Could not build Keys from shared key: {e}"))?;

    // 3. Fetch all gift wraps addressed to this shared key and decrypt them
    let mut messages = fetch_gift_wraps_for_shared_key(&ctx.client, &shared_keys).await?;

    // 4. Apply "since" filter (minutes back from now)
    if *since > 0 {
        let cutoff_ts = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(*since))
            .unwrap()
            .timestamp();
        messages.retain(|(_, ts, _)| (*ts as i64) >= cutoff_ts);
    }

    // 5. Keep only messages sent by the counterparty (not our own side)
    messages.retain(|(_, _, sender_pk)| *sender_pk == pubkey);

    if messages.is_empty() {
        print_no_data_message("📭 No chat messages found for this shared conversation key.");
        return Ok(());
    }

    // 6. Pretty-print the messages
    println!("");
    print_section_header("💬 Shared-Key Chat Messages");

    for (idx, (content, ts, sender_pk)) in messages.iter().enumerate() {
        let date = match chrono::DateTime::from_timestamp(*ts as i64, 0) {
            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "Invalid timestamp".to_string(),
        };

        // Mark messages from the counterparty vs our own future messages (if any)
        let from_label = if *sender_pk == pubkey {
            format!("👤 Counterparty ({sender_pk})")
        } else {
            format!("🧑 You ({sender_pk})")
        };

        println!("📄 Message {}:", idx + 1);
        println!("─────────────────────────────────────");
        println!("⏰ Time: {}", date);
        println!("📨 From: {}", from_label);
        println!("📝 Content:");
        for line in content.lines() {
            println!("   {}", line);
        }
        println!();
    }

    Ok(())
}
