use std::collections::HashSet;

use anyhow::Result;
use base64::engine::general_purpose;
use base64::Engine;
use chrono::DateTime;
use mostro_core::prelude::*;
use nip44::v2::{decrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;

use crate::{
    cli::Context,
    db::{Order, User},
    util::save_order,
};
use sqlx::SqlitePool;

/// Execute logic of command answer
pub async fn print_commands_results(
    message: &MessageKind,
    mut order: Option<Order>,
    ctx: &Context,
) -> Result<()> {
    // Do the logic for the message response
    match message.action {
        Action::NewOrder => {
            if let Some(Payload::Order(order)) = message.payload.as_ref() {
                if let Some(req_id) = message.request_id {
                    if let Err(e) = save_order(
                        order.clone(),
                        &ctx.trade_keys,
                        req_id,
                        ctx.trade_index,
                        &ctx.pool,
                    )
                    .await
                    {
                        return Err(anyhow::anyhow!("Failed to save order: {}", e));
                    }
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("No request id found in message"))
                }
            } else {
                Err(anyhow::anyhow!("No order found in message"))
            }
        }
        // this is the case where the buyer adds an invoice to a takesell order
        Action::WaitingSellerToPay => {
            println!("Now we should wait for the seller to pay the invoice");
            if let Some(mut order) = order.take() {
                match order
                    .set_status(Status::WaitingPayment.to_string())
                    .save(&ctx.pool)
                    .await
                {
                    Ok(_) => println!("Order status updated"),
                    Err(e) => println!("Failed to update order status: {}", e),
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("No order found in message"))
            }
        }
        // this is the case where the buyer adds an invoice to a takesell order
        Action::AddInvoice => {
            if let Some(Payload::Order(order)) = &message.payload {
                println!(
                    "Please add a lightning invoice with amount of {}",
                    order.amount
                );
                if let Some(req_id) = message.request_id {
                    // Save the order
                    if let Err(e) = save_order(
                        order.clone(),
                        &ctx.trade_keys,
                        req_id,
                        ctx.trade_index,
                        &ctx.pool,
                    )
                    .await
                    {
                        return Err(anyhow::anyhow!("Failed to save order: {}", e));
                    }
                } else {
                    return Err(anyhow::anyhow!("No request id found in message"));
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("No order found in message"))
            }
        }
        // this is the case where the buyer pays the invoice coming from a takebuy
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
                if let Some(order) = order {
                    if let Some(req_id) = message.request_id {
                        let store_order = order.clone();
                        // Save the order
                        if let Err(e) = save_order(
                            store_order,
                            &ctx.trade_keys,
                            req_id,
                            ctx.trade_index,
                            &ctx.pool,
                        )
                        .await
                        {
                            println!("Failed to save order: {}", e);
                            return Err(anyhow::anyhow!("Failed to save order: {}", e));
                        }
                    } else {
                        return Err(anyhow::anyhow!("No request id found in message"));
                    }
                } else {
                    return Err(anyhow::anyhow!("No request id found in message"));
                }
            }
            Ok(())
        }
        Action::CantDo => match message.payload {
            Some(Payload::CantDo(Some(
                CantDoReason::OutOfRangeFiatAmount | CantDoReason::OutOfRangeSatsAmount,
            ))) => Err(anyhow::anyhow!(
                "Amount is outside the allowed range. Please check the order's min/max limits."
            )),
            Some(Payload::CantDo(Some(CantDoReason::PendingOrderExists))) => Err(anyhow::anyhow!(
                "A pending order already exists. Please wait for it to be filled or canceled."
            )),
            Some(Payload::CantDo(Some(CantDoReason::InvalidTradeIndex))) => {
                if let Some(order_id) = message.id {
                    let _ = Order::delete_by_id(&ctx.pool, &order_id.to_string()).await;
                }
                // Workaround to update the trade index if mostro is sending this error
                match User::get(&ctx.pool).await {
                    Ok(mut user) => {
                        let new_trade_index = ctx.trade_index + 1;
                        user.set_last_trade_index(new_trade_index);
                        if let Err(e) = user.save(&ctx.pool).await {
                            println!("Failed to update user trade index to continue: {}", e);
                        }
                    }
                    Err(e) => println!(
                        "Failed to get user to update trade index to continue: {}",
                        e
                    ),
                }
                Err(anyhow::anyhow!(
                "Invalid trade index. I have incremented the trade index to the next one to continue - try again to repeat command!"
                ))
            }
            _ => Err(anyhow::anyhow!("Unknown reason: {:?}", message.payload)),
        },
        // this is the case where the user cancels the order
        Action::Canceled => {
            if let Some(order_id) = &message.id {
                // Acquire database connection
                // Verify order exists before deletion
                if Order::get_by_id(&ctx.pool, &order_id.to_string())
                    .await
                    .is_ok()
                {
                    if let Err(e) = Order::delete_by_id(&ctx.pool, &order_id.to_string()).await {
                        return Err(anyhow::anyhow!("Failed to delete order: {}", e));
                    }
                    // Release database connection
                    println!("Order {} canceled!", order_id);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Order not found: {}", order_id))
                }
            } else {
                Err(anyhow::anyhow!("No order id found in message"))
            }
        }
        Action::Rate => {
            println!("Sats released!");
            println!("You can rate the counterpart now");
            Ok(())
        }
        Action::FiatSentOk => {
            if let Some(order_id) = &message.id {
                println!("Fiat sent message for order {:?} received", order_id);
                println!("Waiting for sats release from seller");
                Ok(())
            } else {
                Err(anyhow::anyhow!("No order id found in message"))
            }
        }
        _ => Err(anyhow::anyhow!("Unknown action: {:?}", message.action)),
    }
}

pub async fn parse_dm_events(
    events: Events,
    pubkey: &Keys,
    since: Option<&i64>,
) -> Vec<(Message, u64, PublicKey)> {
    let mut id_set = HashSet::<EventId>::new();
    let mut direct_messages: Vec<(Message, u64, PublicKey)> = Vec::new();

    for dm in events.iter() {
        // Skip if already processed
        if !id_set.insert(dm.id) {
            continue;
        }

        let (created_at, message) = match dm.kind {
            nostr_sdk::Kind::GiftWrap => {
                let unwrapped_gift = match nip59::extract_rumor(pubkey, dm).await {
                    Ok(u) => u,
                    Err(_) => {
                        println!("Error unwrapping gift");
                        continue;
                    }
                };
                let (message, _): (Message, Option<String>) =
                    match serde_json::from_str(&unwrapped_gift.rumor.content) {
                        Ok(msg) => msg,
                        Err(_) => {
                            println!("Error parsing gift wrap content");
                            continue;
                        }
                    };
                (unwrapped_gift.rumor.created_at, message)
            }
            nostr_sdk::Kind::PrivateDirectMessage => {
                let ck = if let Ok(ck) = ConversationKey::derive(pubkey.secret_key(), &dm.pubkey) {
                    ck
                } else {
                    continue;
                };
                let b64decoded_content =
                    match general_purpose::STANDARD.decode(dm.content.as_bytes()) {
                        Ok(b64decoded_content) => b64decoded_content,
                        Err(_) => {
                            continue;
                        }
                    };
                let unencrypted_content = match decrypt_to_bytes(&ck, &b64decoded_content) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        continue;
                    }
                };
                let message_str = match String::from_utf8(unencrypted_content) {
                    Ok(s) => s,
                    Err(_) => {
                        continue;
                    }
                };
                let message = match Message::from_json(&message_str) {
                    Ok(m) => m,
                    Err(_) => {
                        continue;
                    }
                };
                (dm.created_at, message)
            }
            _ => continue,
        };
        // check if the message is older than the since time if it is, skip it
        if let Some(since_time) = since {
            if created_at.as_u64() < *since_time as u64 {
                continue;
            }
        }
        direct_messages.push((message, created_at.as_u64(), dm.pubkey));
    }
    direct_messages.sort_by(|a, b| a.1.cmp(&b.1));
    direct_messages
}

pub async fn print_direct_messages(dm: &[(Message, u64)], pool: &SqlitePool) -> Result<()> {
    if dm.is_empty() {
        println!();
        println!("No new messages");
        println!();
    } else {
        for m in dm.iter() {
            let message = m.0.get_inner_message_kind();
            let date = match DateTime::from_timestamp(m.1 as i64, 0) {
                Some(dt) => dt,
                None => {
                    println!("Error: Invalid timestamp {}", m.1);
                    continue;
                }
            };
            if let Some(order_id) = message.id {
                println!(
                    "Mostro sent you this message for order id: {} at {}",
                    order_id, date
                );
            }
            if let Some(payload) = &message.payload {
                match payload {
                    Payload::PaymentRequest(_, inv, _) => {
                        println!();
                        println!("Pay this invoice to continue --> {}", inv);
                        println!();
                    }
                    Payload::TextMessage(text) => {
                        println!();
                        println!("{text}");
                        println!();
                    }
                    Payload::Dispute(id, info) => {
                        println!("Action: {}", message.action);
                        println!("Dispute id: {}", id);
                        if let Some(info) = info {
                            println!();
                            println!("Dispute info: {:#?}", info);
                            println!();
                        }
                    }
                    Payload::CantDo(Some(cant_do_reason)) => {
                        println!();
                        println!("Error: {:?}", cant_do_reason);
                        println!();
                    }
                    Payload::Order(new_order) if message.action == Action::NewOrder => {
                        if let Some(order_id) = new_order.id {
                            let db_order = Order::get_by_id(pool, &order_id.to_string()).await;
                            if db_order.is_err() {
                                if let Some(trade_index) = message.trade_index {
                                    let trade_keys =
                                        User::get_trade_keys(pool, trade_index).await?;
                                    let _ = Order::new(pool, new_order.clone(), &trade_keys, None)
                                        .await
                                        .map_err(|e| {
                                            anyhow::anyhow!("Failed to create DB order: {:?}", e)
                                        })?;
                                } else {
                                    println!("Warning: No trade_index found for new order");
                                }
                            }
                        }
                        println!();
                        println!("Order: {:#?}", new_order);
                        println!();
                    }
                    _ => {
                        println!();
                        println!("Action: {}", message.action);
                        println!("Payload: {:#?}", message.payload);
                        println!();
                    }
                }
            } else {
                println!();
                println!("Action: {}", message.action);
                println!("Payload: {:#?}", message.payload);
                println!();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {}
