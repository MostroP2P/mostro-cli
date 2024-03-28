use anyhow::Result;
use mostro_core::message::{Action, Message};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::util::{get_keys, send_order_id_cmd};

pub async fn execute_take_buy(
    order_id: &Uuid,
    my_key: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take buy order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );
    let keys = get_keys()?;
    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_string();

    // Create takebuy message
    let take_buy_message =
        Message::new_order(Some(*order_id), Some(master_pubkey), Action::TakeBuy, None)
            .as_json()
            .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, take_buy_message, true).await?;

    Ok(())
}
