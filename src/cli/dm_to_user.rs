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

    let trade_keys = if let Ok(order) = Order::get_by_id(&pool, &order_id.to_string()).await {
        match order.trade_keys.as_ref() {
            Some(trade_keys) => Keys::parse(trade_keys)?,
            None => {
                anyhow::bail!("No trade_keys found for this order");
            }
        }
    } else {
        println!("order {} not found", order_id);
        std::process::exit(0)
    };

    println!("SENDING DM with trade keys: {}", trade_keys.public_key().to_hex());

    send_gift_wrap_dm(client, &trade_keys, &receiver, message).await?;

    Ok(())
}