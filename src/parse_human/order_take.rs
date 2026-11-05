//! Human presentation of `order take-sell` and `order take-buy`: the preview
//! shown before the take and the result after it. Built from the typed input
//! and outcome only; text that came from the node is escaped for the terminal.
//! The invoice the user gave is never shown; the hold invoice the node returns
//! is, alone on its line, because the user has to pay it.

use std::fmt::Write;

use super::table::escape_controls;
use crate::outcome::take::{NextStep, OrderTakeData, OrderTakeInput, TakeSide};

const RULE: &str = "═══════════════════════════════════════";

/// What is about to be sent.
pub fn render_take_preview(
    input: &OrderTakeInput,
    trade_index: &str,
    trade_pubkey: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "🛒 Take Order Preview\n{RULE}");
    let action = match input.side() {
        TakeSide::Sell => "take sell (you buy sats)",
        TakeSide::Buy => "take buy (you sell sats)",
    };
    let _ = writeln!(out, "📈 Action: {action}");
    let _ = writeln!(out, "📋 Order ID: {}", input.order());
    if let Some(amount) = input.amount() {
        let _ = writeln!(out, "💵 Fiat Amount: {amount}");
    }
    if input.has_invoice() {
        let _ = writeln!(out, "🧾 Invoice: provided");
    }
    let _ = writeln!(out, "🔢 Trade Index: {trade_index}");
    let _ = writeln!(out, "🔑 Trade Key: {trade_pubkey}");
    out
}

/// What the node asked the taker to do next.
pub fn render_taken(data: &OrderTakeData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "✅ Order Taken\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", data.order_id());
    if let Some(order) = data.order() {
        let _ = writeln!(out, "💰 Amount: {} sats", order.sats_amount());
        let _ = writeln!(
            out,
            "💱 Fiat: {} {}",
            order.fiat_amount(),
            escape_controls(order.fiat_currency())
        );
        let _ = writeln!(
            out,
            "💳 Payment Method: {}",
            escape_controls(order.payment_method())
        );
    }
    let _ = writeln!(out, "🔢 Trade Index: {}", data.trade_index());
    let _ = writeln!(out, "🔑 Trade Key: {}", data.trade_pubkey());
    match data.next_step() {
        NextStep::AddInvoice => {
            let _ = writeln!(
                out,
                "💡 Next: send your invoice with `order add-invoice --order <id> --invoice <invoice>`."
            );
        }
        NextStep::WaitForSellerPayment => {
            let _ = writeln!(out, "⏳ Waiting for the seller to pay the hold invoice.");
        }
        NextStep::PayBondInvoice => {
            let _ = writeln!(
                out,
                "🪙 The node asks for a taker bond first. Pay this bond invoice with any \
                 Lightning wallet; the take continues once it is paid:"
            );
            let _ = writeln!(
                out,
                "{}",
                escape_controls(data.payment_request().unwrap_or_default())
            );
            let _ = writeln!(
                out,
                "💡 Then read the node's next message to continue (`message list`)."
            );
        }
        NextStep::PayHoldInvoice => {
            let _ = writeln!(out, "⚡ Pay this hold invoice to continue:");
            let _ = writeln!(
                out,
                "{}",
                escape_controls(data.payment_request().unwrap_or_default())
            );
        }
    }
    out
}
