use anyhow::Result;
use mostro_core::Message as MostroMessage;
use mostro_core::{Action, Content};
use nostr_sdk::prelude::ToBech32;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};
use uuid::Uuid;

use crate::util::{get_keys, send_order_id_cmd};

pub async fn execute_take_dispute(
    dispute_id: &Uuid,
    my_key: &Keys,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take dispute {} from mostro pubId {}",
        dispute_id,
        mostro_key.clone()
    );
    let keys = get_keys()?;
    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_bech32()?;
    let content = Some(Content::Dispute(*dispute_id));

    // Create takebuy message
    let take_dispute_message = MostroMessage::new(
        0,
        None,
        Some(master_pubkey),
        Action::AdminTakeDispute,
        content,
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, take_dispute_message, true).await?;

    Ok(())
}
