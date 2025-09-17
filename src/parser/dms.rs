use std::collections::HashSet;

use anyhow::Result;
use base64::engine::general_purpose;
use base64::Engine;
use chrono::DateTime;
use mostro_core::prelude::*;
use nip44::v2::{decrypt_to_bytes, ConversationKey};
use nostr_sdk::prelude::*;

use crate::db::{Order, User};
use sqlx::SqlitePool;

pub async fn parse_dm_events(events: Events, pubkey: &Keys) -> Vec<(Message, u64, PublicKey)> {
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

        let since_time = match chrono::Utc::now().checked_sub_signed(chrono::Duration::minutes(30))
        {
            Some(dt) => dt.timestamp() as u64,
            None => {
                println!("Error: Unable to calculate time 30 minutes ago");
                continue;
            }
        };
        if created_at.as_u64() < since_time {
            continue;
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
