use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

use crate::cli::Context;
use crate::lightning::is_valid_invoice;
use crate::util::{print_dm_events, send_dm, wait_for_dm};

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
    ctx: &Context,
) -> Result<()> {
    let action_name = match action {
        Action::TakeBuy => "take buy",
        Action::TakeSell => "take sell",
        _ => return Err(anyhow::anyhow!("Invalid action for take order")),
    };

    println!(
        "Request of {} order {} from mostro pubId {}",
        action_name, order_id, ctx.mostro_pubkey
    );

    // Create payload based on action type
    let payload = create_take_order_payload(action.clone(), invoice, amount)?;

    // Create request id
    let request_id = Uuid::new_v4().as_u128() as u64;

    // Create message
    let take_order_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        Some(ctx.trade_index),
        action.clone(),
        payload,
    );

    // Send dm to receiver pubkey
    println!(
        "SENDING DM with trade index: {} and trade keys: {:?}",
        ctx.trade_index,
        ctx.trade_keys.public_key().to_hex()
    );

    let message_json = take_order_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send the DM
    // This is so we can wait for the gift wrap event in the main thread
    let sent_message = send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
        &ctx.trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    // Wait for the DM to be sent from mostro
    let recv_event = wait_for_dm(ctx, None, sent_message).await?;

    // Parse the incoming DM
    print_dm_events(recv_event, request_id, ctx).await?;

    Ok(())
}
