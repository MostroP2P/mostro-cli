use crate::util::{print_dm_events, send_dm, wait_for_dm};
use crate::{cli::Context, db::Order, lightning::is_valid_invoice};
use anyhow::Result;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;
use lnurl::lightning_address::LightningAddress;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

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

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(100)
        .set_header(vec![
            Cell::new("Field")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
            Cell::new("Value")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center),
        ]);
    table.add_row(Row::from(vec![
        Cell::new("📋 Order ID"),
        Cell::new(order_id.to_string()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🔑 Trade Keys"),
        Cell::new(order_trade_keys.public_key().to_hex()),
    ]));
    table.add_row(Row::from(vec![
        Cell::new("🎯 Target"),
        Cell::new(ctx.mostro_pubkey.to_string()),
    ]));
    println!("{table}");
    println!("💡 Sending lightning invoice to Mostro...\n");
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

    // Send the DM
    let sent_message = send_dm(
        &ctx.client,
        Some(&ctx.identity_keys),
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
