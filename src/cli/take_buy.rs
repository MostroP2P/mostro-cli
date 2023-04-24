use mostro_core::Action;
use uuid::Uuid;

use anyhow::Result;

use mostro_core::Message as MostroMessage;

use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};

use crate::util::send_order_id_cmd;

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

    // Create takebuy message
    let takebuy_message = MostroMessage::new(0, Some(*order_id), Action::TakeBuy, None)
        .as_json()
        .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, takebuy_message, true).await?;

    Ok(())
}
