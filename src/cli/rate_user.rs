use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

const RATING_BOUNDARIES: [u8; 5] = [1, 2, 3, 4, 5];

use crate::{
    cli::Context,
    db::Order,
    util::{print_dm_events, send_dm, wait_for_dm},
};

// Get the user rate
fn get_user_rate(rating: &u8) -> Result<Payload> {
    if let Some(rating) = RATING_BOUNDARIES.iter().find(|r| r == &rating) {
        Ok(Payload::RatingUser(*rating))
    } else {
        Err(anyhow::anyhow!("Rating must be in the range 1 - 5"))
    }
}

pub async fn execute_rate_user(order_id: &Uuid, rating: &u8, ctx: &Context) -> Result<()> {
    println!("⭐ Rate User");
    println!("═══════════════════════════════════════");
    println!("📋 Order ID: {}", order_id);
    println!("⭐ Rating: {}/5", rating);
    println!("💡 Sending user rating...");
    println!();

    // Check boundaries
    let rating_content = get_user_rate(rating)?;

    // Get the trade keys
    let trade_keys =
        if let Ok(order_to_vote) = Order::get_by_id(&ctx.pool, &order_id.to_string()).await {
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
    )
    .as_json()
    .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    let sent_message = send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &trade_keys,
        &ctx.mostro_pubkey,
        rate_message,
        None,
        false,
    );

    // Wait for incoming DM
    let recv_event = wait_for_dm(ctx, Some(&trade_keys), sent_message).await?;

    // Parse the incoming DM
    // use a fake request id
    let fake_request_id = Uuid::new_v4().as_u128() as u64;
    print_dm_events(recv_event, fake_request_id, ctx, Some(&trade_keys)).await?;

    Ok(())
}
