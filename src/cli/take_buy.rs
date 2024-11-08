use anyhow::Result;
use mostro_core::message::{Action, Content, Message};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::util::send_order_id_cmd;

pub async fn execute_take_buy(
    order_id: &Uuid,
    amount: Option<u32>,
    my_key: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take buy order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );
    // Create takebuy message
    let take_buy_message = Message::new_order(
        None,
        Some(*order_id),
        Action::TakeBuy,
        amount.map(|amt: u32| Content::Amount(amt as i64)),
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, take_buy_message, true, false).await?;

    Ok(())
}
