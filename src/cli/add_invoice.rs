use crate::lightning::is_valid_invoice;
use crate::util::{send_order_id_cmd, sign_content};
use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::message::{Action, Content, Message};
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

pub async fn execute_add_invoice(
    order_id: &Uuid,
    invoice: &str,
    identity_keys: &Keys,
    trade_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    println!(
        "Sending a lightning invoice {} to mostro pubId {}",
        order_id, mostro_key
    );
    let mut content = None;
    // Check invoice string
    let ln_addr = LightningAddress::from_str(invoice);
    if ln_addr.is_ok() {
        content = Some(Content::PaymentRequest(None, invoice.to_string(), None));
    } else {
        match is_valid_invoice(invoice) {
            Ok(i) => content = Some(Content::PaymentRequest(None, i.to_string(), None)),
            Err(e) => println!("{}", e),
        }
    }
    let sig = sign_content(content.clone().unwrap(), trade_keys)?;
    // Create AddInvoice message
    let add_invoice_message = Message::new_order(
        Some(*order_id),
        None,
        None,
        Action::AddInvoice,
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
        add_invoice_message,
        true,
        false,
    )
    .await?;

    Ok(())
}
