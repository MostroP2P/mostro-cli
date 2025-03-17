use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::error::CantDoReason;
use mostro_core::message::{Action, Message, Payload};
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

use crate::db::{connect, Order, User};
use crate::lightning::is_valid_invoice;
use crate::util::send_message_sync;

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

    let dm = send_message_sync(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_sell_message,
        true,
        false,
    )
    .await?;
    let pool = connect().await?;

    let order = dm.iter().find_map(|el| {
        let message = el.0.get_inner_message_kind();
        if message.request_id == Some(request_id) {
            match message.action {
                Action::AddInvoice => {
                    if let Some(Payload::Order(order)) = message.payload.as_ref() {
                        println!(
                            "Please add a lightning invoice with amount of {}",
                            order.amount
                        );
                        return Some(order.clone());
                    }
                }
                Action::CantDo => {
                    if let Some(Payload::CantDo(Some(cant_do_reason))) = &message.payload {
                        match cant_do_reason {
                            CantDoReason::OutOfRangeFiatAmount | CantDoReason::OutOfRangeSatsAmount => {
                                println!("Error: Amount is outside the allowed range. Please check the order's min/max limits.");
                            }
                            _ => {
                                println!("Unknown reason: {:?}", message.payload);
                            }
                        }
                    } else {
                        println!("Unknown reason: {:?}", message.payload);
                        return None;
                    }
                }
                _ => {
                    println!("Unknown action: {:?}", message.action);
                    return None;
                }
            }
        }
        None
    });
    if let Some(o) = order {
        if let Ok(order) = Order::new(&pool, o, trade_keys, Some(request_id as i64)).await {
            if let Some(order_id) = order.id {
                println!("Order {} created", order_id);
            } else {
                println!("Warning: The newly created order has no ID.");
            }
            // Update last trade index to be used in next trade
            match User::get(&pool).await {
                Ok(mut user) => {
                    user.set_last_trade_index(trade_index);
                    if let Err(e) = user.save(&pool).await {
                        println!("Failed to update user: {}", e);
                    }
                }
                Err(e) => println!("Failed to get user: {}", e),
            }
        }
    }

    Ok(())
}
