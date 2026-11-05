//! Human presentation of `order add-invoice` and `bond add-invoice`: the preview
//! shown before the invoice is sent and the result after it. Built from the
//! typed input and outcome only; text that came from the node is escaped for the
//! terminal. The invoice itself is never shown.

use std::fmt::Write;

use super::table::escape_controls;
use crate::outcome::invoice::{
    BondInvoiceData, InvoiceInput, InvoiceKind, InvoiceResult, OrderInvoiceData,
};

const RULE: &str = "═══════════════════════════════════════";

/// What is about to be sent.
pub fn render_invoice_preview(input: &InvoiceInput, trade_pubkey: &str) -> String {
    let mut out = String::new();
    let title = match input.kind() {
        InvoiceKind::Order => "⚡ Add Invoice Preview",
        InvoiceKind::Bond => "🪙 Add Bond Payout Invoice Preview",
    };
    let _ = writeln!(out, "{title}\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", input.order());
    let _ = writeln!(out, "🧾 Invoice: provided");
    let _ = writeln!(out, "🔑 Trade Key: {trade_pubkey}");
    out
}

/// What the node did with the buyer's invoice.
pub fn render_order_invoice(data: &OrderInvoiceData) -> String {
    let mut out = String::new();
    let (title, next) = match data.result() {
        InvoiceResult::WaitingSellerPayment => (
            "✅ Invoice Accepted",
            "⏳ Waiting for the seller to pay the hold invoice.",
        ),
        InvoiceResult::PaymentAccepted => (
            "🎉 Invoice Accepted",
            "✅ The seller's payment is already held; the trade is active.",
        ),
        InvoiceResult::InvoiceUpdated => (
            "✅ Replacement Invoice Stored",
            "💡 The node will use the new invoice for the payout.",
        ),
    };
    let _ = writeln!(out, "{title}\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", data.order_id());
    if let Some(order) = data.order() {
        let status = serde_json::to_value(order.status())
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".to_string());
        let _ = writeln!(out, "📊 Status: {status}");
        let _ = writeln!(out, "💰 Amount: {} sats", order.sats_amount());
        let _ = writeln!(
            out,
            "💱 Fiat: {} {}",
            order.fiat_amount(),
            escape_controls(order.fiat_currency())
        );
    }
    let _ = writeln!(out, "🔑 Trade Key: {}", data.trade_pubkey());
    let _ = writeln!(out, "{next}");
    out
}

/// What the node did with the bond payout invoice.
pub fn render_bond_invoice(data: &BondInvoiceData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "✅ Bond Payout Invoice Accepted\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", data.order_id());
    if let Some(share) = data.counterparty_share_sats() {
        let _ = writeln!(out, "💰 Your share: {share} sats");
    }
    let _ = writeln!(out, "🔑 Trade Key: {}", data.trade_pubkey());
    let _ = writeln!(out, "💡 The node pays the invoice from its wallet.");
    out
}
