//! Human presentation of `order.info`: the familiar labelled lines of the
//! legacy order listing, built from the typed outcome only. Text that came from
//! the node is escaped for the terminal; nothing is invented for a value the
//! node did not send.

use std::fmt::Write;

use super::table::escape_controls;
use crate::outcome::info::{OrderInfoData, OrderRecord};

const RULE: &str = "═══════════════════════════════════════";
const THIN_RULE: &str = "─────────────────────────────────────";

/// The orders the node returned and the ids it did not.
pub fn render_info(data: &OrderInfoData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "📋 Orders List\n{RULE}");
    if data.orders().is_empty() {
        let _ = writeln!(out, "📭 No orders found or unauthorized access");
    } else {
        let _ = writeln!(out, "📊 Found {} order(s):\n", data.orders().len());
        for (index, order) in data.orders().iter().enumerate() {
            order_block(&mut out, index + 1, order);
        }
    }
    if !data.not_returned().is_empty() {
        let _ = writeln!(out, "❔ Not returned by the node:");
        for id in data.not_returned() {
            let _ = writeln!(out, "   {id}");
        }
    }
    out
}

fn order_block(out: &mut String, number: usize, order: &OrderRecord) {
    let _ = writeln!(out, "📄 Order {number}:\n{THIN_RULE}");
    let _ = writeln!(out, "🆔 ID: {}", order.id());
    let _ = writeln!(out, "📈 Kind: {}", wire(order.side()));
    let _ = writeln!(out, "📊 Status: {}", wire(order.status()));
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
    if let Some(line) = time_line("📅 Created", order.created_at(), order.created_at_unix()) {
        let _ = writeln!(out, "{line}");
    }
    if let Some(line) = time_line("⏰ Expires", order.expires_at(), order.expires_at_unix()) {
        let _ = writeln!(out, "{line}");
    }
    out.push('\n');
}

/// `unknown` when the node sent no value, the wire name otherwise.
fn wire<T: serde::Serialize>(value: Option<T>) -> String {
    value
        .and_then(|value| serde_json::to_value(value).ok())
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

/// `<label>: 2026-10-16 00:00:00 UTC`, `Invalid date` when the time exists but
/// has no UTC date, and no line when the node sent no time.
fn time_line(label: &str, text: Option<&str>, unix: Option<&str>) -> Option<String> {
    match (text, unix) {
        (Some(text), _) => {
            let shown = text.replace('T', " ").replace('Z', " UTC");
            Some(format!("{label}: {shown}"))
        }
        (None, Some(_)) => Some(format!("{label}: Invalid date")),
        (None, None) => None,
    }
}
