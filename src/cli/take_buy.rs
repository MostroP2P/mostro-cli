use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::util::{send_dm, wait_for_dm};

pub async fn execute_take_buy(
    order_id: &Uuid,
    amount: Option<u32>,
    identity_keys: &Keys,
    trade_keys: &Keys,
    trade_index: i64,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take buy order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );
    let request_id = Uuid::new_v4().as_u128() as u64;
    let payload = amount.map(|amt: u32| Payload::Amount(amt as i64));
    // Create takebuy message
    let take_buy_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        Some(trade_index),
        Action::TakeBuy,
        payload,
    );

    // Send dm to receiver pubkey
    println!(
        "SENDING DM with trade keys: {:?}",
        trade_keys.public_key().to_hex()
    );

    let message_json = take_buy_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Clone the keys and client for the async call
    let identity_keys = identity_keys.clone();
    let trade_keys_clone = trade_keys.clone();
    let client_clone = client.clone();
    // Subscribe to gift wrap events - ONLY NEW ONES WITH LIMIT 0
    let subscription = Filter::new()
        .pubkey(trade_keys.public_key())
        .kind(nostr_sdk::Kind::GiftWrap)
        .limit(0);

    let opts = SubscribeAutoCloseOptions::default().exit_policy(ReqExitPolicy::WaitForEvents(1));

    client.subscribe(subscription, Some(opts)).await?;

    // Spawn a new task to send the DM
    // This is so we can wait for the gift wrap event in the main thread
    tokio::spawn(async move {
        let _ = send_dm(
            &client_clone,
            Some(&identity_keys.clone()),
            &trade_keys_clone,
            &mostro_key,
            message_json,
            None,
            false,
        )
        .await;
    });

    // Wait for the DM to be sent from mostro
    wait_for_dm(client, trade_keys, request_id, trade_index, None).await?;

    Ok(())
}
