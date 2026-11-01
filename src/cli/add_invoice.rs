use crate::parser::common::{
    create_emoji_field_row, create_field_value_header, create_standard_table,
};
use crate::util::{print_dm_events, send_dm, wait_for_dm};
use crate::{cli::Context, db::Order, lightning::is_valid_invoice};
use anyhow::Result;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

/// The payload of `add-invoice`: a Lightning address as given, or a BOLT-11
/// invoice that parses and is not expired, in its canonical form.
fn invoice_payload(invoice: &str) -> Result<Payload> {
    if LightningAddress::from_str(invoice).is_ok() {
        return Ok(Payload::PaymentRequest(None, invoice.to_string(), None));
    }
    match is_valid_invoice(invoice) {
        Ok(i) => Ok(Payload::PaymentRequest(None, i.to_string(), None)),
        Err(e) => Err(anyhow::anyhow!("Invalid invoice: {}", e)),
    }
}

pub async fn execute_add_invoice(order_id: &Uuid, invoice: &str, ctx: &Context) -> Result<()> {
    // Get order from order id
    let order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;
    // Get trade keys of specific order
    let trade_keys = order
        .trade_keys
        .clone()
        .ok_or(anyhow::anyhow!("Missing trade keys"))?;

    let order_trade_keys = Keys::parse(&trade_keys)?;

    println!("⚡ Add Lightning Invoice");
    println!("═══════════════════════════════════════");

    let mut table = create_standard_table();
    table.set_header(create_field_value_header());
    table.add_row(create_emoji_field_row(
        "📋 ",
        "Order ID",
        &order_id.to_string(),
    ));
    table.add_row(create_emoji_field_row(
        "🔑 ",
        "Trade Keys",
        &order_trade_keys.public_key().to_hex(),
    ));
    table.add_row(create_emoji_field_row(
        "🎯 ",
        "Target",
        &ctx.mostro_pubkey.to_string(),
    ));
    println!("{table}");
    println!("💡 Sending lightning invoice to Mostro...\n");
    let payload = Some(invoice_payload(invoice)?);

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

    // Send the DM
    let sent_message = send_dm(
        &ctx.client,
        &ctx.identity_keys,
        &order_trade_keys,
        &ctx.mostro_pubkey,
        message_json,
        None,
        false,
    );

    // Wait for the DM to be sent from mostro
    let recv_event = wait_for_dm(ctx, Some(&order_trade_keys), sent_message).await?;

    // Parse the incoming DM
    print_dm_events(recv_event, request_id, ctx, Some(&order_trade_keys)).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(payload: Payload) -> (Option<SmallOrder>, String, Option<i64>) {
        match payload {
            Payload::PaymentRequest(order, invoice, amount) => (order, invoice, amount),
            other => panic!("a payment request, got {other:?}"),
        }
    }

    #[test]
    fn a_lightning_address_is_sent_as_given_in_a_bare_payment_request() {
        let (order, invoice, amount) = request(invoice_payload("buyer@example.com").unwrap());

        assert_eq!(
            (order.is_none(), invoice.as_str(), amount),
            (true, "buyer@example.com", None)
        );
    }

    #[test]
    fn anything_that_is_neither_an_address_nor_a_valid_invoice_is_refused_locally() {
        for bad in [
            "",
            "not an invoice",
            "lnbc1garbage",
            "buyer@",
            "@example.com",
        ] {
            let error = invoice_payload(bad).unwrap_err().to_string();
            assert!(error.starts_with("Invalid invoice: "), "{bad}: {error}");
        }
    }
}
