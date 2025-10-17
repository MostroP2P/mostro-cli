use crate::{db::Order, util::send_gift_wrap_dm};
use anyhow::Result;
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

    // Send the DM
    println!("💬 Direct Message to User");
    println!("═══════════════════════════════════════");
    println!("📋 Order ID: {}", order_id);
    println!("🔑 Trade Keys: {}", trade_keys.public_key().to_hex());
    println!("🎯 Recipient: {}", receiver);
    println!("💬 Message: {}", message);
    println!("💡 Sending gift wrap message...");
    println!();

    send_gift_wrap_dm(client, &trade_keys, &receiver, message).await?;

    println!("✅ Gift wrap message sent successfully!");

    Ok(())
}
