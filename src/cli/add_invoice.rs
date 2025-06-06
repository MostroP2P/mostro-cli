use crate::db::connect;
use crate::util::{send_dm, wait_for_dm};
use crate::{db::Order, lightning::is_valid_invoice};
use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

pub async fn execute_add_invoice(
    order_id: &Uuid,
    invoice: &str,
    identity_keys: &Keys,
    mostro_key: PublicKey,
    client: &Client,
) -> Result<()> {
    let pool = connect().await?;
    let order = Order::get_by_id(&pool, &order_id.to_string()).await?;
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
    let request_id = Uuid::new_v4().as_u128() as u64;
    // Create AddInvoice message
    let add_invoice_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        None,
        Action::AddInvoice,
        payload,
    );

    let message_json = add_invoice_message
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;

    // Clone the keys and client for the async call
    let identity_keys = identity_keys.clone();
    let trade_keys_clone = trade_keys.clone();
    let client_clone = client.clone();

    // Spawn a new task to send the DM
    // This is so we can wait for the gift wrap event in the main thread
    tokio::spawn(async move {
        if let Err(e) = send_dm(
            &client_clone,
            Some(&identity_keys.clone()),
            &trade_keys_clone,
            &mostro_key,
            message_json,
            None,
            false,
        )
        .await
        {
            eprintln!("Failed to send DM: {}", e);
        }
    });

    // Wait for the DM to be sent from mostro
    wait_for_dm(client, &trade_keys, request_id, 0, Some(order)).await?;

    Ok(())
}
