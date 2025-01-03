use std::collections::HashSet;

use anyhow::Result;
use chrono::DateTime;
use mostro_core::message::{Action, Message, Payload};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::{
    db::{connect, Order, User},
    util::{get_direct_messages, send_message_sync},
};

pub async fn execute_get_dm(
    since: &i64,
    identity_keys: Keys,
    trade_keys: Keys,
    trade_index: i64,
    mostro_key: PublicKey,
    client: &Client,
    from_user: bool,
) -> Result<()> {
    let mut dm: Vec<(Message, u64)> = Vec::new();
    let pool = connect().await?;
    let orders = Order::get_all(&pool).await.unwrap();
    let trade_keys_hex = trade_keys.secret_key().to_secret_hex();
    let order_trade_keys = orders
        .iter()
        .filter_map(|order| order.trade_keys.as_ref().cloned())
        .collect::<Vec<String>>();
    let mut unique_trade_keys = order_trade_keys
        .iter()
        .cloned()
        .collect::<HashSet<String>>();
    unique_trade_keys.insert(trade_keys_hex);
    let final_trade_keys = unique_trade_keys.iter().cloned().collect::<Vec<String>>();
    for keys in final_trade_keys.iter() {
        let trade_keys =
            Keys::parse(keys).map_err(|e| anyhow::anyhow!("Failed to parse trade keys: {}", e))?;
        let dm_temp = get_direct_messages(client, &trade_keys, *since, from_user).await;
        dm.extend(dm_temp);
    }

    if dm.is_empty() {
        println!();
        println!("No new messages");
        println!();
    } else {
        for m in dm.iter() {
            let message = m.0.get_inner_message_kind();
            let date = DateTime::from_timestamp(m.1 as i64, 0).unwrap();
            if message.id.is_some() {
                println!(
                    "Mostro sent you this message for order id: {} at {}",
                    m.0.get_inner_message_kind().id.unwrap(),
                    date
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
                    Payload::CantDo(Some(cant_do_reason)) => {
                        println!();
                        println!("Error: {:?}", cant_do_reason);
                        println!();
                    }
                    Payload::Order(new_order) if message.action == Action::NewOrder => {
                        let db_order =
                            Order::get_by_id(&pool, &new_order.id.unwrap().to_string()).await;
                        if db_order.is_err() {
                            let _ = Order::new(&pool, new_order.clone(), &trade_keys, None)
                                .await
                                .map_err(|e| {
                                    anyhow::anyhow!("Failed to create DB order: {:?}", e)
                                })?;
                            // Update last trade index
                            match User::get(&pool).await {
                                Ok(mut user) => {
                                    user.set_last_trade_index(trade_index);
                                    if let Err(e) = user.save(&pool).await {
                                        println!("Failed to update user: {}", e);
                                    }
                                }
                                Err(e) => println!("Failed to get user: {}", e),
                            }
                            let request_id = Uuid::new_v4().as_u128() as u64;
                            // Now we send a message to Mostro letting it know the trade key and
                            // trade index for this new order
                            let message = Message::new_order(
                                Some(new_order.id.unwrap()),
                                Some(request_id),
                                Some(trade_index),
                                Action::TradePubkey,
                                None,
                            );
                            let _ = send_message_sync(
                                client,
                                Some(&identity_keys),
                                &trade_keys,
                                mostro_key,
                                message,
                                false,
                                false,
                            )
                            .await?;
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
