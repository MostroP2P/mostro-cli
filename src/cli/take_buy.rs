use anyhow::Result;
use mostro_core::message::{Action, Message, Payload};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::{
    db::{connect, User},
    util::send_order_id_cmd,
};

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
    let payload = amount.map(|amt: u32| Payload::Amount(amt as i64));
    // Create takebuy message
    let take_buy_message = Message::new_order(
        Some(*order_id),
        None,
        Some(trade_index.into()),
        Action::TakeBuy,
        payload,
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(
        client,
        Some(identity_keys),
        trade_keys,
        mostro_key,
        take_buy_message,
        true,
        false,
    )
    .await?;

    let pool = connect().await?;
    // Update last trade index
    let mut user = User::get(&pool).await.unwrap();
    user.set_last_trade_index(trade_index as i64);
    user.save(&pool).await.unwrap();

    Ok(())
}
