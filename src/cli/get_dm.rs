use std::collections::HashSet;

use anyhow::Result;
use mostro_core::message::{Message, Payload};
use nostr_sdk::prelude::*;

use crate::{
    db::{connect, Order},
    util::get_direct_messages,
};

pub async fn execute_get_dm(
    since: &i64,
    trade_keys: Keys,
    client: &Client,
    from_user: bool,
) -> Result<()> {
    let mut dm: Vec<(String, String, u64)> = Vec::new();
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
        let trade_keys = Keys::parse(keys).unwrap();
        let dm_temp = get_direct_messages(client, &trade_keys, *since, from_user).await;
        dm.extend(dm_temp);
    }

    if dm.is_empty() {
        println!();
        println!("No new messages");
        println!();
    } else {
        for el in dm.iter() {
            match Message::from_json(&el.0) {
                Ok(m) => {
                    if m.get_inner_message_kind().id.is_some() {
                        println!(
                            "Mostro sent you this message for order id: {} at {}",
                            m.get_inner_message_kind().id.unwrap(),
                            el.1
                        );
                    }
                    if let Some(Payload::PaymentRequest(_, inv, _)) =
                        &m.get_inner_message_kind().payload
                    {
                        println!();
                        println!("Pay this invoice to continue --> {}", inv);
                        println!();
                    } else if let Some(Payload::TextMessage(text)) =
                        &m.get_inner_message_kind().payload
                    {
                        println!();
                        println!("{text}");
                        println!();
                    } else {
                        println!();
                        println!("Action: {}", m.get_inner_message_kind().action);
                        println!("Payload: {:#?}", m.get_inner_message_kind().payload);
                        println!();
                    }
                }
                Err(_) => {
                    println!("You got this message:");
                    println!();
                    println!("{}", el.0);
                    println!();
                }
            }
        }
    }
    Ok(())
}
