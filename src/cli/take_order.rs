use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

use crate::lightning::is_valid_invoice;
use crate::util;

/// Create payload based on action type and parameters
fn create_take_order_payload(
    action: Action,
    invoice: &Option<String>,
    amount: Option<u32>,
) -> Result<Option<Payload>> {
    match action {
        Action::TakeBuy => Ok(amount.map(|amt: u32| Payload::Amount(amt as i64))),
        Action::TakeSell => Ok(Some(match invoice {
            Some(inv) => {
                let initial_payload = match LightningAddress::from_str(inv) {
                    Ok(_) => Payload::PaymentRequest(None, inv.to_string(), None),
                    Err(_) => match is_valid_invoice(inv) {
                        Ok(i) => Payload::PaymentRequest(None, i.to_string(), None),
                        Err(e) => {
                            println!("{}", e);
                            Payload::PaymentRequest(None, inv.to_string(), None)
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
        })),
        _ => Err(anyhow::anyhow!("Invalid action for take order")),
    }
}

/// Unified function to handle both take buy and take sell orders
#[allow(clippy::too_many_arguments)]
pub async fn execute_take_order(
    order_id: &Uuid,
    action: Action,
    invoice: &Option<String>,
    amount: Option<u32>,
    identity_keys: &Keys,
    trade_keys: &Keys,
    trade_index: i64,
    mostro_key: PublicKey,
    client: &Client,
    pool: &SqlitePool,
) -> Result<()> {
    let action_name = match action {
        Action::TakeBuy => "take buy",
        Action::TakeSell => "take sell",
        _ => return Err(anyhow::anyhow!("Invalid action for take order")),
    };

    println!(
        "Request of {} order {} from mostro pubId {}",
        action_name,
        order_id,
        mostro_key.clone()
    );

    // Create payload based on action type
    let payload = create_take_order_payload(action.clone(), invoice, amount)?;

    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create message
    let take_order_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        Some(trade_index),
        action.clone(),
        payload,
    );

    // Send dm to receiver pubkey
    println!(
        "SENDING DM with trade keys: {:?}",
        trade_keys.public_key().to_hex()
    );

    let message_json = take_order_message
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
        let _ = util::send_dm(
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

    // For take_sell, add an additional subscription with timestamp filtering
    if action == Action::TakeSell {
        let subscription = Filter::new()
            .pubkey(trade_keys.public_key())
            .kind(nostr_sdk::Kind::GiftWrap)
            .since(Timestamp::from(chrono::Utc::now().timestamp() as u64))
            .limit(0);

        client.subscribe(subscription, None).await?;
    }

    // Wait for the DM to be sent from mostro
    util::wait_for_dm(
        client,
        trade_keys,
        request_id,
        Some(trade_index),
        None,
        pool,
    )
    .await?;

    Ok(())
}
