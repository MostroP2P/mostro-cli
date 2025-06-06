use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;

use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

use crate::lightning::is_valid_invoice;
use crate::util::{send_dm, wait_for_dm};

#[allow(clippy::too_many_arguments)]
pub async fn execute_take_sell(
    order_id: &Uuid,
    invoice: &Option<String>,
    amount: Option<u32>,
    identity_keys: &Keys,
    trade_keys: &Keys,
    trade_index: i64,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take sell order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );

    let payload = match invoice {
        Some(inv) => {
            let initial_payload = match LightningAddress::from_str(inv) {
                Ok(_) => Payload::PaymentRequest(None, inv.to_string(), None),
                Err(_) => match is_valid_invoice(inv) {
                    Ok(i) => Payload::PaymentRequest(None, i.to_string(), None),
                    Err(e) => {
                        println!("{}", e);
                        Payload::PaymentRequest(None, inv.to_string(), None) // or handle error differently
                    }
                },
            };

            match amount {
                Some(amt) => match initial_payload {
                    Payload::PaymentRequest(a, b, _) => {
                        Payload::PaymentRequest(a, b, Some(amt as i64))
                    }
                    payload => payload,
                },
                None => initial_payload,
            }
        }
        None => amount
            .map(|amt| Payload::Amount(amt.into()))
            .unwrap_or(Payload::Amount(0)),
    };

    let request_id = Uuid::new_v4().as_u128() as u64;
    // Create takesell message
    let take_sell_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        Some(trade_index),
        Action::TakeSell,
        Some(payload),
    );

    // Send dm to receiver pubkey
    println!(
        "SENDING DM with trade keys: {:?}",
        trade_keys.public_key().to_hex()
    );
    let message_json = take_sell_message
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

    // Subscribe to gift wrap events - ONLY NEW ONES WITH LIMIT 0
    let subscription = Filter::new()
        .pubkey(trade_keys.public_key())
        .kind(nostr_sdk::Kind::GiftWrap)
        .since(Timestamp::from(chrono::Utc::now().timestamp() as u64))
        .limit(2);

    client.subscribe(subscription, None).await?;

    // Wait for the DM to be sent from mostro
    wait_for_dm(client, trade_keys, request_id, trade_index, None).await?;

    Ok(())
}
