//! `declarepayer` and `paymenthistory`: payer declaration and
//! payment-account history (protocol book, `payer_declaration.md`).

use anyhow::Result;
use mostro_core::prelude::*;
use nostr_sdk::prelude::*;
use uuid::Uuid;

use crate::{
    cli::Context,
    db::Order,
    parser::common::{print_info_line, print_key_value, print_section_header},
    util::{
        payer::{canonical_payer, PayerMethod},
        print_dm_events, send_dm, wait_for_dm,
    },
};

/// Trade keys stored for `order_id`.
async fn order_trade_keys(order_id: &Uuid, ctx: &Context) -> Result<Keys> {
    let order = Order::get_by_id(&ctx.pool, &order_id.to_string())
        .await
        .map_err(|_| anyhow::anyhow!("order {} not found", order_id))?;
    match order.trade_keys.as_ref() {
        Some(keys) => Ok(Keys::parse(keys)?),
        None => Err(anyhow::anyhow!("No trade_keys found for this order")),
    }
}

/// Send `kind` for `order_id` signed with the order's trade keys and print
/// Mostro's answer.
async fn send_and_print(
    order_id: &Uuid,
    action: Action,
    payload: Option<Payload>,
    ctx: &Context,
) -> Result<()> {
    let trade_keys = order_trade_keys(order_id, ctx).await?;
    let request_id = Uuid::new_v4().as_u128() as u64;
    let message = Message::new_order(Some(*order_id), Some(request_id), None, action, payload)
        .as_json()
        .map_err(|_| anyhow::anyhow!("Failed to serialize message"))?;
    let sent = send_dm(
        &ctx.client,
        &ctx.identity_keys,
        &trade_keys,
        &ctx.mostro_pubkey,
        message,
        None,
        false,
    );
    let recv_event = wait_for_dm(ctx, Some(&trade_keys), sent).await?;
    print_dm_events(recv_event, request_id, ctx, Some(&trade_keys)).await?;
    Ok(())
}

/// Buyer: canonicalise the payer account, declare its hash for the order
/// and show the canonical string to send to the seller over the chat.
pub async fn execute_declare_payer(
    order_id: &Uuid,
    method: &str,
    fields: &[String],
    ctx: &Context,
) -> Result<()> {
    let method = PayerMethod::parse(method)?;
    let canonical = canonical_payer(method, fields)?;
    let hash = payment_hash(&canonical);

    print_section_header("🧾 Declare Payer");
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_key_value("🏦", "Method", method.prefix());
    print_key_value("🔤", "Canonical", &canonical);
    print_key_value("#️⃣", "Payment hash", &hash);
    print_info_line(
        "💡",
        "Only the hash goes to Mostro. Send the canonical string to the seller over the chat.",
    );
    println!();

    let payload = Payload::PayerDeclaration(PayerDeclaration::new(hash));
    send_and_print(order_id, Action::DeclarePayer, Some(payload), ctx).await
}

/// Seller: ask Mostro for the history of the account the buyer declared.
pub async fn execute_payment_history(order_id: &Uuid, ctx: &Context) -> Result<()> {
    print_section_header("📈 Payment History");
    print_key_value("📋", "Order ID", &order_id.to_string());
    print_info_line(
        "💡",
        "Asking Mostro for the buyer's payment-account history...",
    );
    println!();
    send_and_print(order_id, Action::PaymentHistory, None, ctx).await
}
