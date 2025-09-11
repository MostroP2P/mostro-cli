use crate::util::{send_dm, wait_for_dm};
use crate::{db::Order, lightning::is_valid_invoice};
use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

pub async fn execute_add_invoice(
    order_id: &Uuid,
    invoice: &str,
    identity_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
    pool: &SqlitePool,
) -> Result<()> {
    println!("Adding invoice to order {}", order_id);
    let order = Order::get_by_id(pool, &order_id.to_string()).await?;
    let trade_keys = order
        .trade_keys
        .clone()
        .ok_or(anyhow::anyhow!("Missing trade keys"))?;
    let trade_keys = Keys::parse(&trade_keys)?;

    println!(
        "Sending a lightning invoice {} to mostro pubId {}",
        order_id, mostro_key
    );
    // Check invoice string
    let ln_addr = LightningAddress::from_str(invoice);
    let payload = if ln_addr.is_ok() {
        Some(Payload::PaymentRequest(None, invoice.to_string(), None))
    } else {
        match is_valid_invoice(invoice) {
            Ok(i) => Some(Payload::PaymentRequest(None, i.to_string(), None)),
            Err(e) => {
                println!("Invalid invoice: {}", e);
                None
            }
        }
    };

    // Create request id
    let request_id = Uuid::new_v4().as_u128() as u64;
    // Create AddInvoice message
    let add_invoice_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        None,
        Action::AddInvoice,
        payload,
    );

    // Serialize the message
    let message_json = add_invoice_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Send DM
    send_dm(
        client,
        Some(identity_keys),
        &trade_keys,
        &mostro_key,
        message_json,
        None,
        false,
    )
    .await?;

    // Wait for the DM to be sent from mostro and update the order
    wait_for_dm(client, &trade_keys, request_id, None, Some(order), pool).await?;
    Ok(())
}
