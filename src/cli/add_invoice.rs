use crate::db::connect;
use crate::util::send_message_sync;
use crate::{db::Order, lightning::is_valid_invoice};
use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::message::{Action, Message, Payload};
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
    let order = Order::get_by_id(&pool, &order_id.to_string())
        .await
        .unwrap();
    let trade_keys = order.trade_keys.unwrap();
    let trade_keys = Keys::parse(trade_keys).unwrap();

    println!(
        "Sending a lightning invoice {} to mostro pubId {}",
        order_id, mostro_key
    );
    let mut payload = None;
    // Check invoice string
    let ln_addr = LightningAddress::from_str(invoice);
    if ln_addr.is_ok() {
        payload = Some(Payload::PaymentRequest(None, invoice.to_string(), None));
    } else {
        match is_valid_invoice(invoice) {
            Ok(i) => payload = Some(Payload::PaymentRequest(None, i.to_string(), None)),
            Err(e) => println!("{}", e),
        }
    }
    // Create AddInvoice message
    let add_invoice_message =
        Message::new_order(Some(*order_id), None, None, Action::AddInvoice, payload);

    send_message_sync(
        client,
        Some(identity_keys),
        &trade_keys,
        mostro_key,
        add_invoice_message,
        true,
        false,
    )
    .await?;

    Ok(())
}
