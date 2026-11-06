//! Human presentation of `order mine`: one line per order of the local store.
//! Built from the typed outcome only, which holds public values only.

use std::fmt::Write;

use crate::outcome::mine::OrderMineData;

const RULE: &str = "═══════════════════════════════════════";

/// The orders of the local store, newest first.
pub fn render_mine(data: &OrderMineData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "📒 Local Orders\n{RULE}");
    if data.orders().is_empty() {
        let _ = writeln!(out, "No local orders.");
        return out;
    }
    let _ = writeln!(out, "{} order(s), newest first:", data.orders().len());
    for order in data.orders() {
        let _ = writeln!(
            out,
            "📋 {}  {}  {}  index {}  key {}",
            order.id(),
            order.side(),
            order.status(),
            order.trade_index().unwrap_or("unknown"),
            order.trade_pubkey().unwrap_or("none")
        );
    }
    out
}
