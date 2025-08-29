use crate::{db::Order, util::send_gift_wrap_dm};
use anyhow::Result;
use nostr_sdk::prelude::*;
use uuid::Uuid;

pub async fn execute_dm_to_user(
    receiver: PublicKey,
    client: &Client,
    order_id: &Uuid,
    message: &str,
) -> Result<()> {
    let pool = crate::db::connect().await?;

    let order = Order::get_by_id(&pool, &order_id.to_string())
        .await
        .map_err(|_| anyhow::anyhow!("order {} not found", order_id))?;
    let trade_keys = match order.trade_keys.as_ref() {
        Some(trade_keys) => Keys::parse(trade_keys)?,
        None => anyhow::bail!("No trade_keys found for this order"),
    };

    println!("SENDING DM with trade keys: {}", trade_keys.public_key().to_hex());

    send_gift_wrap_dm(client, &trade_keys, &receiver, message).await?;

    Ok(())
}