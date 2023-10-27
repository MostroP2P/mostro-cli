use anyhow::Result;
use mostro_core::Message as MostroMessage;
use mostro_core::{Action, Content};
use nostr_sdk::prelude::ToBech32;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};
use uuid::Uuid;

use crate::lightning::is_valid_invoice;
use crate::util::{get_keys, send_order_id_cmd};

pub async fn execute_take_sell(
    order_id: &Uuid,
    invoice: &Option<String>,
    my_key: &Keys,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Request of take sell order {} from mostro pubId {}",
        order_id,
        mostro_key.clone()
    );
    let mut content = None;
    if invoice.is_some() {
        // Check invoice string
        let valid_invoice = is_valid_invoice(invoice.as_ref().unwrap());
        match valid_invoice {
            Ok(i) => content = Some(Content::PaymentRequest(None, i.to_string())),
            Err(e) => panic!("{}", e),
        }
    }
    let keys = get_keys()?;
    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_bech32()?;

    // Create takesell message
    let take_sell_message = MostroMessage::new(
        0,
        Some(*order_id),
        Some(master_pubkey),
        Action::TakeSell,
        content,
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, take_sell_message, true).await?;
    Ok(())
}
