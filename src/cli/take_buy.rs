use anyhow::Result;
use mostro_core::Message as MostroMessage;
use mostro_core::{Action, Content, Peer};
use nostr_sdk::prelude::ToBech32;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};
use uuid::Uuid;

use crate::util::{get_keys, send_order_id_cmd};

pub async fn execute_take_buy(
    order_id: &Uuid,
    my_key: &Keys,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take buy order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );
    let keys = get_keys()?;
    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_bech32()?;
    let peer = Peer::new(master_pubkey.clone());
    let content = Some(Content::Peer(peer));

    // Create takebuy message
    let takebuy_message = MostroMessage::new(
        0,
        Some(*order_id),
        Some(master_pubkey),
        Action::TakeBuy,
        content,
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, takebuy_message, true).await?;

    Ok(())
}
