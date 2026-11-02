//! Human presentation of `order.sync`: the orders that were given a local row,
//! with the key index that owns each, and a count of what was read. Built from
//! the typed outcome only; text that came from the node is escaped for the
//! terminal.

use std::fmt::Write;

use super::table::escape_controls;
use crate::outcome::info::OrderRecord;
use crate::outcome::sync::{AdoptedOrder, SyncData};

const RULE: &str = "═══════════════════════════════════════";
const THIN_RULE: &str = "─────────────────────────────────────";

/// What was found and saved.
pub fn render_sync(data: &SyncData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "🔄 Order Sync\n{RULE}");
    let _ = writeln!(
        out,
        "⏰ Last {} minutes, {} trade key(s) read, {} message(s) from Mostro ({} ignored)",
        data.since_minutes(),
        data.keys_read(),
        data.messages_read(),
        data.messages_ignored()
    );
    if data.keys_not_read() != "0" {
        let _ = writeln!(
            out,
            "⚠️  {} older trade key(s) not read",
            data.keys_not_read()
        );
    }
    if data.adopted().is_empty() {
        let _ = writeln!(
            out,
            "✅ Nothing to add: every announced order is already known."
        );
    } else {
        let _ = writeln!(out, "📥 {} order(s) added:\n", data.adopted().len());
        for adopted in data.adopted() {
            block(&mut out, adopted);
        }
        let _ = writeln!(
            out,
            "💡 You can act on them now, for example `order cancel --order <id>`."
        );
    }
    if data.truncated() {
        let _ = writeln!(
            out,
            "⚠️  There may be more messages than read; narrow the window with --since."
        );
    }
    out
}

fn block(out: &mut String, adopted: &AdoptedOrder) {
    let order: &OrderRecord = adopted.order();
    let _ = writeln!(out, "📋 Order ID: {}\n{THIN_RULE}", order.id());
    let word = |value: Option<serde_json::Value>| {
        value
            .as_ref()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".to_string())
    };
    let _ = writeln!(
        out,
        "📈 Side: {}",
        word(serde_json::to_value(order.side()).ok())
    );
    let _ = writeln!(
        out,
        "📊 Status: {}",
        word(serde_json::to_value(order.status()).ok())
    );
    let _ = writeln!(out, "💰 Amount: {} sats", order.sats_amount());
    match (order.fiat_amount_min(), order.fiat_amount_max()) {
        (Some(min), Some(max)) => {
            let _ = writeln!(
                out,
                "💱 Fiat: {min}-{max} {}",
                escape_controls(order.fiat_currency())
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "💱 Fiat: {} {}",
                order.fiat_amount(),
                escape_controls(order.fiat_currency())
            );
        }
    }
    let _ = writeln!(out, "🔢 Trade Index: {}", adopted.trade_index());
    let _ = writeln!(out, "🔑 Trade Key: {}\n", adopted.trade_pubkey());
}
