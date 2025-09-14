use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use uuid::Uuid;

const RATING_BOUNDARIES: [u8; 5] = [1, 2, 3, 4, 5];

use crate::{db::Order, util::send_message_sync};

// Get the user rate
fn get_user_rate(rating: &u8) -> Result<Payload> {
    if let Some(rating) = RATING_BOUNDARIES.iter().find(|r| r == &rating) {
        Ok(Payload::RatingUser(*rating))
    } else {
        Err(anyhow::anyhow!("Rating must be in the range 1 - 5"))
    }
}

pub async fn execute_rate_user(
    order_id: &Uuid,
    rating: &u8,
    identity_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
    pool: &SqlitePool,
) -> Result<()> {
    // Check boundaries
    let rating_content = get_user_rate(rating)?;

    // Get the trade keys
    let trade_keys = if let Ok(order_to_vote) = Order::get_by_id(pool, &order_id.to_string()).await
    {
        match order_to_vote.trade_keys.as_ref() {
            Some(trade_keys) => Keys::parse(trade_keys)?,
            None => {
                return Err(anyhow::anyhow!("No trade_keys found for this order"));
            }
        }
    } else {
        return Err(anyhow::anyhow!("order {} not found", order_id));
    };

    // Create rating message of counterpart
    let rate_message = Message::new_order(
        Some(*order_id),
        None,
        None,
        Action::RateUser,
        Some(rating_content),
    );

    send_message_sync(
        client,
        Some(identity_keys),
        &trade_keys,
        mostro_key,
        rate_message,
        true,
        false,
    )
    .await?;

    Ok(())
}
