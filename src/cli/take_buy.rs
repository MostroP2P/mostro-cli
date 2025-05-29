use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::{
    db::{connect, Order, User},
    util::{get_direct_messages, send_message_sync},
};

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

    let mut attempts = 0;
    let max_attempts = 10;
    let mut order = None;
    // Send dm to receiver pubkey
    println!(
        "SENDING DM with trade keys: {:?}",
        trade_keys.public_key().to_hex()
    );

    send_message_sync(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_buy_message.clone(),
        true,
        false,
    )
    .await?;

    while attempts < max_attempts {
        let dm = get_direct_messages(client, trade_keys, 15, false).await;
        order = dm.iter().find_map(|el| {
            let message = el.0.get_inner_message_kind();
            if message.request_id == Some(request_id) {
                match message.action {
                    Action::PayInvoice => {
                        if let Some(Payload::PaymentRequest(order, invoice, _)) = &message.payload {
                            println!(
                                "Mostro sent you this hold invoice for order id: {}",
                                order
                                    .as_ref()
                                    .and_then(|o| o.id)
                                    .map_or("unknown".to_string(), |id| id.to_string())
                            );
                            println!();
                            println!("Pay this invoice to continue -->  {}", invoice);
                            println!();
                            return order.clone();
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

        if order.is_some() {
            break; // Exit the loop if an order is found
        } else {
            print!("#");
        }

        attempts += 1;
    }

    let pool = connect().await?;

    if let Some(o) = order {
        match Order::new(&pool, o, trade_keys, Some(request_id as i64)).await {
            Ok(order) => {
                println!("Order {} created", order.id.unwrap());
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
            Err(e) => println!("{}", e),
        }
    }

    Ok(())
}
