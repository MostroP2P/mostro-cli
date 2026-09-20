use crate::cli::Context;
use crate::db::Order;
use crate::parser::common::{
    print_info_line, print_key_value, print_no_data_message, print_section_header,
};
use crate::util::FETCH_EVENTS_TIMEOUT;
use anyhow::Result;
use mostro_core::chat::{
    chat_filter, derive_chat_keys, unwrap_chat_message, CHAT_DEFAULT_LOOKBACK_SECS,
};
use nostr_sdk::prelude::*;
use uuid::Uuid;

/// Fetch user-to-user chat messages (kind 14, protocol#52).
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
    execute_get_dm_user_labelled(pubkey, order_id, since, "Counterparty", ctx).await
}

/// Same, with a name for who is on the other end. The dispute chat uses the
/// identical key derivation but the other side is a solver, not the peer.
pub async fn execute_get_dm_user_labelled(
    pubkey: PublicKey,
    order_id: Uuid,
    since: &i64,
    label: &str,
    ctx: &Context,
) -> Result<()> {
    print_section_header("📨 Fetch User Direct Messages");
    print_key_value("👥", label, &pubkey.to_string());
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_key_value("⏰", "Since", &format!("{} minutes ago", since));
    print_info_line("💡", "Fetching chat messages...");
    println!();

    let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load order {order_id}: {e}"))?;

    let trade_keys_str = order
        .trade_keys
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Missing trade keys for order {order_id}"))?;
    let trade_keys =
        Keys::parse(&trade_keys_str).map_err(|e| anyhow::anyhow!("Invalid trade keys: {e}"))?;

    let max_minutes: i64 = (CHAT_DEFAULT_LOOKBACK_SECS / 60) as i64;
    if *since > max_minutes {
        return Err(anyhow::anyhow!(
            "Lookback window is limited to 7 days ({} minutes); requested {} minutes",
            max_minutes,
            since
        ));
    }

    let (conv, sign) = derive_chat_keys(&trade_keys, &pubkey)
        .map_err(|e| anyhow::anyhow!("Failed to derive chat keys: {e}"))?;
    let sign_pubkey = sign.public_key();
    let allowed_signers = [trade_keys.public_key(), pubkey];
    // chat_filter defaults to a seven-day lookback. For a shorter `since`,
    // narrow it here so the relays and the decrypt loop skip events that
    // would only be discarded later. Safe for kind 14: created_at is the
    // real send time.
    let mut filter = chat_filter(sign_pubkey);
    if *since > 0 {
        if let Some(cutoff) =
            chrono::Utc::now().checked_sub_signed(chrono::Duration::minutes(*since))
        {
            filter = filter.since(Timestamp::from(cutoff.timestamp() as u64));
        }
    }
    let events = ctx
        .client
        .fetch_events(filter, FETCH_EVENTS_TIMEOUT)
        .await
        .map_err(|e| anyhow::anyhow!("could not read this conversation: {e}"))?;

    let now = Timestamp::now();
    let mut messages: Vec<(String, i64, PublicKey)> = Vec::new();
    for outer in events.iter() {
        match unwrap_chat_message(&conv, &sign_pubkey, &allowed_signers, outer, now) {
            Ok(chat) => {
                messages.push((chat.content, chat.created_at.as_secs() as i64, chat.sender))
            }
            Err(e) => log::debug!("skipping chat event {}: {e}", outer.id),
        }
    }

    if *since > 0 {
        let cutoff_ts = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(*since))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Invalid 'since' value {}; could not compute cutoff timestamp",
                    since
                )
            })?
            .timestamp();
        messages.retain(|(_, ts, _)| (*ts) >= cutoff_ts);
    }

    messages.retain(|(_, _, sender_pk)| *sender_pk == pubkey);

    if messages.is_empty() {
        print_no_data_message("📭 No chat messages found for this conversation.");
        return Ok(());
    }

    print_section_header("💬 Chat Messages");

    for (idx, (content, ts, sender_pk)) in messages.iter().enumerate() {
        let date = match chrono::DateTime::from_timestamp(*ts, 0) {
            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "Invalid timestamp".to_string(),
        };

        println!("📄 Message {}:", idx + 1);
        println!("─────────────────────────────────────");
        println!("⏰ Time: {}", date);
        println!("📨 From: 👤 Counterparty ({sender_pk})");
        println!("📝 Content:");
        for line in content.lines() {
            println!("   {}", line);
        }
        println!();
    }

    Ok(())
}
