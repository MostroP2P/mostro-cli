//! Human presentation of `order new`: the preview shown before the order is
//! created and the result after it. Built from the typed input and outcome
//! only; text that came from the node is escaped for the terminal.

use std::fmt::Write;

use mostro_core::order::Kind;

use super::table::escape_controls;
use crate::outcome::info::OrderRecord;
use crate::outcome::new::{OrderNewData, OrderNewInput};

const RULE: &str = "═══════════════════════════════════════";

/// What is about to be sent. The invoice itself is never shown.
pub fn render_preview(input: &OrderNewInput, trade_index: &str, trade_pubkey: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "🆕 New Order Preview\n{RULE}");
    let side = match input.side() {
        Kind::Buy => "buy",
        Kind::Sell => "sell",
    };
    let _ = writeln!(out, "📈 Kind: {side}");
    let _ = writeln!(out, "💱 Fiat Code: {}", input.currency());
    match input.fiat_amount_max() {
        Some(max) => {
            let _ = writeln!(out, "📊 Fiat Range: {}-{max}", input.fiat_amount());
        }
        None => {
            let _ = writeln!(out, "💵 Fiat Amount: {}", input.fiat_amount());
        }
    }
    if input.sats() == 0 {
        let _ = writeln!(out, "💰 Amount: market price");
    } else {
        let _ = writeln!(out, "💰 Amount: {} sats", input.sats());
    }
    let _ = writeln!(
        out,
        "💳 Payment Method: {}",
        escape_controls(input.payment_method())
    );
    let _ = writeln!(out, "📊 Premium: {}%", input.premium());
    if input.has_invoice() {
        let _ = writeln!(out, "🧾 Invoice: provided");
    }
    if input.expiration_days() > 0 {
        let _ = writeln!(out, "⏰ Expires in: {} day(s)", input.expiration_days());
    }
    let _ = writeln!(out, "🔢 Trade Index: {trade_index}");
    let _ = writeln!(out, "🔑 Trade Key: {trade_pubkey}");
    out
}

/// The order the node created.
pub fn render_new(data: &OrderNewData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "✅ Order Created\n{RULE}");
    let order: &OrderRecord = data.order();
    let _ = writeln!(out, "🆔 Order ID: {}", order.id());
    let _ = writeln!(out, "📊 Status: {}", wire(order.status()));
    let _ = writeln!(out, "📈 Kind: {}", wire(order.side()));
    let _ = writeln!(out, "💰 Amount: {} sats", order.sats_amount());
    let _ = writeln!(
        out,
        "💱 Fiat Code: {}",
        escape_controls(order.fiat_currency())
    );
    match (order.fiat_amount_min(), order.fiat_amount_max()) {
        (None, None) => {
            let _ = writeln!(out, "💵 Fiat Amount: {}", order.fiat_amount());
        }
        (min, max) => {
            let _ = writeln!(
                out,
                "💵 Fiat Range: {}-{}",
                min.unwrap_or("?"),
                max.unwrap_or("?")
            );
        }
    }
    let _ = writeln!(
        out,
        "💳 Payment Method: {}",
        escape_controls(order.payment_method())
    );
    let _ = writeln!(out, "📊 Premium: {}%", order.premium_percent());
    let _ = writeln!(out, "🔢 Trade Index: {}", data.trade_index());
    let _ = writeln!(out, "🔑 Trade Key: {}", data.trade_pubkey());
    out
}

/// `unknown` when the node sent no value, the wire name otherwise.
fn wire<T: serde::Serialize>(value: Option<T>) -> String {
    value
        .and_then(|value| serde_json::to_value(value).ok())
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
