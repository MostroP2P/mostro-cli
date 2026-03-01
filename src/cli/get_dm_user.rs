use crate::cli::Context;
use crate::db::Order;
use crate::parser::common::{
    print_info_line, print_key_value, print_no_data_message, print_section_header,
};
use crate::parser::dms::print_direct_messages;
use crate::util::messaging::{derive_shared_key_bytes, fetch_gift_wraps_for_shared_key};
use crate::util::{fetch_events_list, Event, ListKind};
use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;

pub async fn execute_get_dm_user(since: &i64, ctx: &Context) -> Result<()> {
    // Get all trade keys from orders
    let mut trade_keys_hex = Order::get_all_trade_keys(&ctx.pool).await?;

    // Include Mostro pubkey so we also fetch messages addressed to Mostro
    let admin_pubkey_hex = ctx.mostro_pubkey.to_hex();
    if !trade_keys_hex.iter().any(|k| k == &admin_pubkey_hex) {
        trade_keys_hex.push(admin_pubkey_hex);
    }
    // De-duplicate any repeated keys coming from DB/admin
    trade_keys_hex.sort();
    trade_keys_hex.dedup();

    // Check if the trade keys are empty
    if trade_keys_hex.is_empty() {
        print_no_data_message("No trade keys found in orders");
        return Ok(());
    }

    print_section_header("📨 Fetch User Direct Messages");
    print_key_value(
        "🔍",
        "Searching for DMs in trade keys",
        &format!("{}", trade_keys_hex.len()),
    );
    print_key_value("⏰", "Since", &format!("{} minutes ago", since));
    print_info_line("💡", "Fetching direct messages...");
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

    // Extract (Message, u64, PublicKey) tuples from Event::MessageTuple variants (classic DMs)
    let mut dm_events: Vec<(Message, u64, PublicKey)> = Vec::new();
    for event in direct_messages {
        if let Event::MessageTuple(tuple) = event {
            dm_events.push(*tuple);
        }
    }

    // Also fetch and decrypt shared-key custom wraps (shared key = trade_keys + identity_keys)
    let trade_keys_hex_list = Order::get_all_trade_keys(&ctx.pool).await?;
    let identity_pubkey = ctx.identity_keys.public_key();
    for trade_hex in trade_keys_hex_list {
        let trade_keys = match Keys::parse(&trade_hex) {
            Ok(k) => k,
            Err(e) => {
                log::warn!("get_dm_user: could not parse trade_keys: {}", e);
                continue;
            }
        };
        let shared_key = match derive_shared_key_bytes(&trade_keys, &identity_pubkey) {
            Ok(b) => b,
            Err(e) => {
                log::warn!(
                    "get_dm_user: could not derive shared key (trade + identity): {}",
                    e
                );
                continue;
            }
        };
        let shared_keys = match SecretKey::from_slice(&shared_key) {
            Ok(sk) => Keys::new(sk),
            Err(e) => {
                log::warn!("get_dm_user: could not build Keys from shared key: {}", e);
                continue;
            }
        };
        let shared_msgs = match fetch_gift_wraps_for_shared_key(&ctx.client, &shared_keys).await {
            Ok(m) => m,
            Err(e) => {
                log::warn!(
                    "get_dm_user: failed to fetch gift wraps for shared key: {}",
                    e
                );
                continue;
            }
        };
        for (content, ts, sender_pubkey) in shared_msgs {
            let parsed: (Message, Option<String>) = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("get_dm_user: could not parse shared-key DM content: {}", e);
                    continue;
                }
            };
            dm_events.push((parsed.0, ts as u64, sender_pubkey));
        }

        // Also fetch shared-key wraps for (trade_keys + mostro_pubkey) so we see admin replies
        // (send_admin_dm_attach uses that derivation when we send to admin; admin uses same key to reply)
        let shared_key_admin = match derive_shared_key_bytes(&trade_keys, &ctx.mostro_pubkey) {
            Ok(b) => b,
            Err(e) => {
                log::warn!(
                    "get_dm_user: could not derive shared key (trade + mostro): {}",
                    e
                );
                continue;
            }
        };
        let shared_keys_admin = match SecretKey::from_slice(&shared_key_admin) {
            Ok(sk) => Keys::new(sk),
            Err(e) => {
                log::warn!("get_dm_user: could not build Keys from shared key (admin): {}", e);
                continue;
            }
        };
        if let Ok(admin_msgs) = fetch_gift_wraps_for_shared_key(&ctx.client, &shared_keys_admin).await {
            for (content, ts, sender_pubkey) in admin_msgs {
                if let Ok((parsed, _)) = serde_json::from_str::<(Message, Option<String>)>(&content) {
                    dm_events.push((parsed, ts as u64, sender_pubkey));
                }
            }
        }
    }

    if dm_events.is_empty() {
        print_no_data_message("You don't have any direct messages in your trade keys");
        return Ok(());
    }

    print_direct_messages(&dm_events, Some(ctx.mostro_pubkey)).await?;
    Ok(())
}
