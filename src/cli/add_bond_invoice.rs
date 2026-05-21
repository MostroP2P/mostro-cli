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

/// Reply to a Mostro `add-bond-invoice` request: the non-slashed counterparty
/// provides a bolt11 sized at their share of a slashed bond.
///
/// This is the inbound `add-bond-invoice` request's dual — Mostro asks for a
/// bolt11 (carried as [`Payload::BondPayoutRequest`]) and we answer with the
/// invoice in the standard [`Payload::PaymentRequest`] shape, signed with the
/// order's trade key. See the protocol's "Bond payout invoice" action.
pub async fn execute_add_bond_invoice(order_id: &Uuid, invoice: &str, ctx: &Context) -> Result<()> {
    // Get order from order id
    let order = Order::get_by_id(&ctx.pool, &order_id.to_string()).await?;
    // Get trade keys of specific order (the non-slashed counterparty side)
    let trade_keys = order
        .trade_keys
        .clone()
        .ok_or(anyhow::anyhow!("Missing trade keys"))?;

    let order_trade_keys = Keys::parse(&trade_keys)?;

    println!("🪙 Add Bond Payout Invoice");
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
    println!("💡 Sending bond payout invoice to Mostro...\n");
    // Parse invoice (Lightning address or BOLT11) and build payload
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
    // Create AddBondInvoice reply message
    let add_bond_invoice_message = Message::new_order(
        Some(*order_id),
        Some(request_id),
        None,
        Action::AddBondInvoice,
        payload,
    );

    // Serialize the message
    let message_json = add_bond_invoice_message
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

    // Wait for a possible reply. On success Mostro pays the invoice from its
    // wallet without acknowledging over Nostr, so a timeout here is the happy
    // path; Mostro only answers with `cant-do` on failure (late reply, wrong
    // sender, bad invoice, etc.).
    match wait_for_dm(ctx, Some(&order_trade_keys), sent_message).await {
        Ok(recv_event) => {
            print_dm_events(recv_event, request_id, ctx, Some(&order_trade_keys)).await?;
        }
        Err(_) => {
            println!("✅ Bond payout invoice submitted to Mostro.");
            println!("💡 Mostro will pay it from its wallet; no further confirmation is sent.");
            println!("💡 Run `get-dm` to check for a `cant-do` response in case of an error.");
        }
    }

    Ok(())
}
