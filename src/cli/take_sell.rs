use mostro_core::{Action, Content};
use uuid::Uuid;

use anyhow::Result;

use mostro_core::Message as MostroMessage;

use nostr_sdk::secp256k1::XOnlyPublicKey;
use nostr_sdk::{Client, Keys};

use crate::lightning::is_valid_invoice;

use crate::util::send_order_id_cmd;

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
            Err(e) => println!("{}", e),
        }
    }

    // Create takesell message
    let takesell_message = MostroMessage::new(0, Some(*order_id), Action::TakeSell, content)
        .as_json()
        .unwrap();

    send_order_id_cmd(client, my_key, mostro_key, takesell_message, true).await?;
    Ok(())
}
