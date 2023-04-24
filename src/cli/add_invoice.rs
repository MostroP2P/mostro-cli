use mostro_core::{Action, Content};
use uuid::Uuid;

use anyhow::Result;

use mostro_core::Message as MostroMessage;

use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};

use crate::lightning::is_valid_invoice;

use crate::util::send_order_id_cmd;

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

    // Create AddInvoice message
    let add_invoice_message = MostroMessage::new(0, Some(*order_id), Action::AddInvoice, content)
        .as_json()
        .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, add_invoice_message, true).await?;

    Ok(())
}
