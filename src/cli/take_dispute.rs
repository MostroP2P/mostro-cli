use anyhow::Result;
use mostro_core::message::{Action, Message};
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::util::send_order_id_cmd;

pub async fn execute_take_dispute(
    dispute_id: &Uuid,
    my_key: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take dispute {} from mostro pubId {}",
        dispute_id,
        mostro_key.clone()
    );
    // Create takebuy message
    let take_dispute_message =
        Message::new_dispute(Some(*dispute_id), Action::AdminTakeDispute, None)
            .as_json()
            .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, take_dispute_message, true, true).await?;

    Ok(())
}
