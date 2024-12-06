use anyhow::Result;
use mostro_core::message::{Action, Content, Message};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::util::{send_order_id_cmd, sign_content};

pub async fn execute_take_buy(
    order_id: &Uuid,
    amount: Option<u32>,
    identity_keys: &Keys,
    trade_keys: &Keys,
    trade_index: u32,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take buy order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );
    let content = amount.map(|amt: u32| Content::Amount(amt as i64));
    let sig = sign_content(content.clone().unwrap(), trade_keys)?;
    // Create takebuy message
    let take_buy_message = Message::new_order(
        Some(*order_id),
        None,
        Some(trade_index),
        Action::TakeBuy,
        content,
        Some(sig),
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(
        client,
        identity_keys,
        trade_keys,
        mostro_key,
        take_buy_message,
        true,
        false,
    )
    .await?;

    Ok(())
}
