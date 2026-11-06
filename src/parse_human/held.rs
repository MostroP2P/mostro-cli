//! Human presentation of `dispute open` and `order rate`: the preview shown
//! before the request and the result after it. Built from the typed input and
//! outcome only; nothing secret is shown.

use std::fmt::Write;

use uuid::Uuid;

use crate::outcome::dispute::DisputeOpenData;
use crate::outcome::rate::{RateData, RateInput};

const RULE: &str = "═══════════════════════════════════════";

/// What `dispute open` is about to send.
pub fn render_dispute_preview(order: Uuid, trade_pubkey: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "⚖️  Open Dispute Preview\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {order}");
    let _ = writeln!(out, "🔑 Trade Key: {trade_pubkey}");
    let _ = writeln!(
        out,
        "💡 A solver will take the dispute and contact both parties."
    );
    out
}

/// The dispute the node opened.
pub fn render_dispute(data: &DisputeOpenData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "⚖️  Dispute Opened\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", data.order_id());
    let _ = writeln!(out, "🆔 Dispute ID: {}", data.dispute_id());
    let _ = writeln!(out, "🔑 Trade Key: {}", data.trade_pubkey());
    let _ = writeln!(
        out,
        "💡 Wait for a solver; `message list --order <id>` shows when one takes it."
    );
    out
}

/// What `order rate` is about to send.
pub fn render_rate_preview(input: &RateInput, trade_pubkey: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "⭐ Rate Counterparty Preview\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", input.order());
    let _ = writeln!(out, "⭐ Rating: {}/5", input.rating());
    let _ = writeln!(out, "🔑 Trade Key: {trade_pubkey}");
    out
}

/// The rating the node received.
pub fn render_rate(data: &RateData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "⭐ Rating Received\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", data.order_id());
    let _ = writeln!(out, "⭐ Rating: {}/5", data.rating());
    let _ = writeln!(out, "🔑 Trade Key: {}", data.trade_pubkey());
    out
}
