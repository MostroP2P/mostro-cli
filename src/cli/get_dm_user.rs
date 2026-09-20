use crate::cli::Context;
use crate::db::Order;
use crate::parser::common::{
    print_info_line, print_key_value, print_no_data_message, print_section_header,
};
use crate::util::messaging::{derive_shared_key_bytes, fetch_gift_wraps_for_shared_key};
use crate::util::FETCH_EVENTS_TIMEOUT;
use anyhow::Result;
use mostro_core::chat::{chat_filter, derive_chat_keys, unwrap_chat_message};
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

    // 3. Enforce the 7-day lookback window used by fetch_gift_wraps_for_shared_key
    let max_minutes: i64 = 7 * 24 * 60;
    if *since > max_minutes {
        return Err(anyhow::anyhow!(
            "Lookback window is limited to 7 days ({} minutes); requested {} minutes",
            max_minutes,
            since
        ));
    }

    // 4a. Current envelope (protocol#52): kind 14 signed by K_sign, payload
    // encrypted to K_conv. This is what the mobile app and Mostrix publish.
    let mut messages: Vec<(String, i64, PublicKey)> = Vec::new();
    // Wat er misging, per envelop. Zie de afhandeling onder 4b.
    let mut mislukt: Vec<String> = Vec::new();
    match derive_chat_keys(&trade_keys, &pubkey) {
        Ok((conv, sign)) => {
            let sign_pubkey = sign.public_key();
            let allowed_signers = [trade_keys.public_key(), pubkey];
            // chat_filter defaults to a seven-day lookback. For a shorter
            // `since`, narrow it here so the relays and the decrypt loop skip
            // events that step 5 would only discard again. Safe for kind 14:
            // unlike a gift wrap its created_at is the real send time.
            let mut filter = chat_filter(sign_pubkey);
            if *since > 0 {
                if let Some(cutoff) =
                    chrono::Utc::now().checked_sub_signed(chrono::Duration::minutes(*since))
                {
                    filter = filter.since(Timestamp::from(cutoff.timestamp() as u64));
                }
            }
            match ctx.client.fetch_events(filter, FETCH_EVENTS_TIMEOUT).await {
                Ok(events) => {
                    let now = Timestamp::now();
                    for outer in events.iter() {
                        match unwrap_chat_message(&conv, &sign_pubkey, &allowed_signers, outer, now)
                        {
                            Ok(chat) => messages.push((
                                chat.content,
                                chat.created_at.as_secs() as i64,
                                chat.sender,
                            )),
                            // Not addressed to us, replayed, or malformed: skip quietly.
                            Err(e) => log::debug!("skipping chat event {}: {e}", outer.id),
                        }
                    }
                }
                Err(e) => mislukt.push(format!("kind 14 envelope: {e}")),
            }
        }
        Err(e) => mislukt.push(format!("chat keys: {e}")),
    }

    // 4b. Legacy gift-wrap envelope, so conversations started before the
    // migration stay readable (mostro-core keeps this path for dual-read).
    if !crate::cli::dm_to_user::geen_legacy() {
        match fetch_gift_wraps_for_shared_key(&ctx.client, &shared_keys).await {
            Ok(gevonden) => messages.extend(gevonden),
            Err(e) => mislukt.push(format!("legacy gift wrap: {e}")),
        }
    }

    // One broken half must not throw away what the other half found, and it
    // must not be silent either: a conversation that came back short for
    // technical reasons looks exactly like a counterparty with little to say.
    if !mislukt.is_empty() {
        if messages.is_empty() {
            return Err(anyhow::anyhow!(
                "could not read this conversation: {}",
                mislukt.join("; ")
            ));
        }
        print_info_line(
            "⚠️",
            &format!(
                "Part of this conversation could not be read ({}). What follows may be incomplete.",
                mislukt.join("; ")
            ),
        );
    }

    // 5. Apply "since" filter (minutes back from now)
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

    // 6. Keep only messages sent by the counterparty (not our own side)
    messages.retain(|(_, _, sender_pk)| *sender_pk == pubkey);

    if messages.is_empty() {
        print_no_data_message("📭 No chat messages found for this shared conversation key.");
        return Ok(());
    }

    // 7. Pretty-print the messages
    print_section_header("💬 Shared-Key Chat Messages");

    for (idx, (content, ts, sender_pk)) in messages.iter().enumerate() {
        let date = match chrono::DateTime::from_timestamp(*ts, 0) {
            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "Invalid timestamp".to_string(),
        };

        // Messages are already filtered to only those from the counterparty.
        let from_label = format!("👤 Counterparty ({sender_pk})");

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
