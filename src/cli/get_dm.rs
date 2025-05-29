use anyhow::Result;
use chrono::DateTime;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use crate::cli::MOSTRO_KEYS;

use crate::{
    db::{connect, Order, User},
    util::get_direct_messages,
};

pub async fn execute_get_dm(
    since: &i64,
    trade_index: i64,
    client: &Client,
    from_user: bool,
    admin: bool,
    mostro_pubkey: &PublicKey,
) -> Result<()> {
    let mut dm: Vec<(Message, u64)> = Vec::new();
    let pool = connect().await?;
    if !admin {
        for index in 1..=trade_index {
            let keys = User::get_trade_keys(&pool, index).await?;
            let dm_temp =
                get_direct_messages(client, &keys, *since, from_user, Some(mostro_pubkey)).await;
            dm.extend(dm_temp);
        }
    } else {
        let dm_temp = get_direct_messages(client,   MOSTRO_KEYS.get().unwrap(), *since, from_user).await;
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
                        if new_order.id.is_some() {
                            let db_order =
                                Order::get_by_id(&pool, &new_order.id.unwrap().to_string()).await;
                            if db_order.is_err() {
                                let trade_index = message.trade_index.unwrap();
                                let trade_keys = User::get_trade_keys(&pool, trade_index).await?;
                                let _ = Order::new(&pool, new_order.clone(), &trade_keys, None)
                                    .await
                                    .map_err(|e| {
                                        anyhow::anyhow!("Failed to create DB order: {:?}", e)
                                    })?;
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
