use crate::parser::common::{
    print_info_line, print_key_value, print_section_header, print_success_message,
};
use crate::{
    db::Order,
    util::{derive_shared_keys, send_admin_chat_message_via_shared_key},
};
use anyhow::Result;
use mostro_core::chat::{derive_chat_keys, wrap_chat_message};
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn execute_dm_to_user(
    receiver: PublicKey,
    client: &Client,
    order_id: &Uuid,
    message: &str,
    pool: &SqlitePool,
) -> Result<()> {
    // Get the order
    let order = Order::get_by_id(pool, &order_id.to_string())
        .await
        .map_err(|_| anyhow::anyhow!("order {} not found", order_id))?;
    // Get the trade keys
    let trade_keys = match order.trade_keys.as_ref() {
        Some(trade_keys) => Keys::parse(trade_keys)?,
        None => anyhow::bail!("No trade_keys found for this order"),
    };

    // Derive per-dispute shared keys between our trade keys and the receiver pubkey
    let shared_keys = derive_shared_keys(Some(&trade_keys), Some(&receiver))
        .ok_or_else(|| anyhow::anyhow!("Failed to derive shared key for this DM"))?;

    // Print summary and send shared-key wrap DM
    print_section_header("💬 Direct Message to User");
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_key_value("🔑", "Trade Keys", &trade_keys.public_key().to_hex());
    print_key_value("🎯", "Recipient", &receiver.to_string());
    print_key_value("💬", "Message", message);
    print_key_value(
        "🔑",
        "Shared Key Pubkey",
        &shared_keys.public_key().to_hex(),
    );
    print_info_line("💡", "Sending chat message...");
    println!();

    // Current envelope (protocol#52): kind 14 signed by K_sign, payload
    // encrypted to K_conv. This is what the mobile app and Mostrix read.
    let (conv, sign) = derive_chat_keys(&trade_keys, &receiver)
        .map_err(|e| anyhow::anyhow!("Failed to derive chat keys: {e}"))?;
    let event = wrap_chat_message(&trade_keys, &conv, &sign, message)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wrap chat message: {e}"))?;
    client.send_event(&event).await?;

    // Also publish the legacy gift wrap, so a counterparty still running a
    // pre-migration client keeps receiving messages during the transition.
    send_admin_chat_message_via_shared_key(client, &trade_keys, &shared_keys, message).await?;

    print_success_message("Chat message sent (kind 14 envelope + legacy gift wrap)!");

    Ok(())
}
