use crate::parser::dms::print_commands_results;
use crate::util::{fetch_events_list, send_dm, Event, ListKind};
use crate::{cli::Context, db::Order, lightning::is_valid_invoice};
use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

pub async fn execute_add_invoice(order_id: &Uuid, invoice: &str, ctx: &Context) -> Result<()> {
    let order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;
    let trade_keys = order
        .trade_keys
        .clone()
        .ok_or(anyhow::anyhow!("Missing trade keys"))?;
    let order_trade_keys = Keys::parse(&trade_keys)?;
    println!(
        "Order trade keys: {:?}",
        order_trade_keys.public_key().to_hex()
    );

    println!(
        "Sending a lightning invoice for order {} to mostro pubId {}",
        order_id, ctx.mostro_pubkey
    );
    // Check invoice string
    let ln_addr = LightningAddress::from_str(invoice);
    let payload = if ln_addr.is_ok() {
        Some(Payload::PaymentRequest(None, invoice.to_string(), None))
    } else {
        match is_valid_invoice(invoice) {
            Ok(i) => Some(Payload::PaymentRequest(None, i.to_string(), None)),
            Err(e) => {
                return Err(anyhow::anyhow!("Invalid invoice: {}", e));
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

    // Clone the keys and client for the async call
    let identity_keys_clone = ctx.identity_keys.clone();
    let client_clone = ctx.client.clone();
    let mostro_pubkey_clone = ctx.mostro_pubkey;
    let order_trade_keys_clone = order_trade_keys.clone();

    // Spawn a new task to send the DM
    // This is so we can wait for the gift wrap event in the main thread
    tokio::spawn(async move {
        let _ = send_dm(
            &client_clone,
            Some(&identity_keys_clone),
            &order_trade_keys,
            &mostro_pubkey_clone,
            message_json,
            None,
            false,
        )
        .await;
    });

    let events = fetch_events_list(
        ListKind::WaitForUpdate,
        None,
        None,
        None,
        ctx,
        Some(&order_trade_keys_clone),
        None,
    )
    .await?;

    // We just need the first event
    let recv_event = events.first().unwrap();
    if let Event::MessageTuple(tuple) = recv_event {
        let message = tuple.0.get_inner_message_kind();
        if message.request_id == Some(request_id) {
            let _ = print_commands_results(message, Some(order.clone()), ctx).await;
        }
    }

    Ok(())
}
