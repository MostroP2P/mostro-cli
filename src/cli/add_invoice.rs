use crate::lightning::is_valid_invoice;
use crate::util::{get_keys, send_order_id_cmd};
use anyhow::Result;
use mostro_core::Message as MostroMessage;
use mostro_core::{Action, Content};
use nostr_sdk::prelude::ToBech32;
use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};
use uuid::Uuid;

pub async fn execute_add_invoice(
    order_id: &Uuid,
    invoice: &str,
    my_key: &Keys,
    mostro_key: XOnlyPublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Sending a lightning invoice {} to mostro pubId {}",
        order_id, mostro_key
    );
    let mut content = None;
    // Check invoice string
    let valid_invoice = is_valid_invoice(invoice);
    match valid_invoice {
        Ok(i) => content = Some(Content::PaymentRequest(None, i.to_string())),
        Err(e) => println!("{}", e),
    }
    let keys = get_keys()?;
    // This should be the master pubkey
    let master_pubkey = keys.public_key().to_bech32()?;
    // Create AddInvoice message
    let add_invoice_message = MostroMessage::new(
        0,
        Some(*order_id),
        master_pubkey,
        Action::AddInvoice,
        content,
    )
    .as_json()
    .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, add_invoice_message, true).await?;

    Ok(())
}
