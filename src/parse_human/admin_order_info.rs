//! Human presentation of `admin.order-info`: the order as the node stores it,
//! then its escrow, its payout and its bonds, each only when the node reported
//! it. Built from the typed outcome only. Every text that came from the node,
//! the statuses and states included, is escaped for the terminal.

use std::fmt::Write;

use super::order_info::time_line;
use super::table::escape_controls;
use crate::outcome::admin_order_info::{
    AdminOrderInfoData, BondRecord, CashuEscrowRecord, HoldInvoiceRecord, PayoutRecord,
};

const RULE: &str = "═══════════════════════════════════════";
const THIN_RULE: &str = "─────────────────────────────────────";

/// The node's snapshot of one order.
pub fn render_admin_order_info(data: &AdminOrderInfoData) -> String {
    let mut out = String::new();
    order_block(&mut out, data);
    if let Some(hold) = &data.hold_invoice {
        hold_invoice_block(&mut out, hold);
    }
    if let Some(escrow) = &data.cashu_escrow {
        cashu_escrow_block(&mut out, escrow);
    }
    if let Some(payout) = &data.payout {
        payout_block(&mut out, payout);
    }
    if !data.bonds.is_empty() {
        bonds_block(&mut out, &data.bonds);
    }
    out
}

fn order_block(out: &mut String, data: &AdminOrderInfoData) {
    let _ = writeln!(out, "🔎 Order {}\n{RULE}", data.order_id);
    let _ = writeln!(out, "📈 Kind: {}", escape_controls(&data.side));
    let _ = writeln!(out, "📊 Status: {}", escape_controls(&data.status));
    let _ = writeln!(out, "💰 Amount: {} sats", data.sats_amount);
    let _ = writeln!(out, "🧾 Fee: {} sats", data.fee_sats);
    let _ = writeln!(out, "🧭 Routing Fee: {} sats", data.routing_fee_sats);
    let _ = writeln!(
        out,
        "💱 Fiat Code: {}",
        escape_controls(&data.fiat_currency)
    );
    match (&data.fiat_amount_min, &data.fiat_amount_max) {
        (None, None) => {
            let _ = writeln!(out, "💵 Fiat Amount: {}", data.fiat_amount);
        }
        (min, max) => {
            let _ = writeln!(
                out,
                "💵 Fiat Amount: {} (range {}-{})",
                data.fiat_amount,
                min.as_deref().unwrap_or("?"),
                max.as_deref().unwrap_or("?")
            );
        }
    }
    let _ = writeln!(
        out,
        "💳 Payment Method: {}",
        escape_controls(&data.payment_method)
    );
    let _ = writeln!(out, "📊 Premium: {}%", data.premium_percent);
    if let Some(parent) = &data.range_parent_id {
        let _ = writeln!(out, "🧩 Range Parent: {parent}");
    }
    time(out, "📅 Created", &data.created_at, &data.created_at_unix);
    time(out, "🤝 Taken", &data.taken_at, &data.taken_at_unix);
    time(out, "⏰ Expires", &data.expires_at, &data.expires_at_unix);
    let _ = writeln!(out, "🟢 Buyer: {}", key(&data.buyer_trade_pubkey));
    let _ = writeln!(out, "🔴 Seller: {}", key(&data.seller_trade_pubkey));
    if let Some(who) = sides(data.buyer_dispute, data.seller_dispute) {
        let _ = writeln!(out, "⚖️  Dispute opened by: {who}");
    }
    if let Some(who) = sides(
        data.buyer_cooperative_cancel,
        data.seller_cooperative_cancel,
    ) {
        let _ = writeln!(out, "🚫 Cooperative cancel asked by: {who}");
    }
    let _ = writeln!(
        out,
        "💸 Failed payment: {} (attempts: {})",
        if data.failed_payment { "yes" } else { "no" },
        data.payment_attempts
    );
    time(
        out,
        "🔒 Hold settled",
        &data.hold_settled_at,
        &data.hold_settled_at_unix,
    );
}

fn hold_invoice_block(out: &mut String, hold: &HoldInvoiceRecord) {
    let _ = writeln!(out, "\n🧱 Escrow: hold invoice\n{THIN_RULE}");
    let _ = writeln!(out, "State: {}", state(&hold.state));
    time(out, "Held", &hold.held_at, &hold.held_at_unix);
}

fn cashu_escrow_block(out: &mut String, escrow: &CashuEscrowRecord) {
    let _ = writeln!(out, "\n🧱 Escrow: Cashu\n{THIN_RULE}");
    let _ = writeln!(out, "Mint: {}", escape_controls(&escrow.mint_url));
    time(out, "Locked", &escrow.locked_at, &escrow.locked_at_unix);
}

fn payout_block(out: &mut String, payout: &PayoutRecord) {
    let _ = writeln!(out, "\n⚡ Payout to the buyer\n{THIN_RULE}");
    let _ = writeln!(out, "State: {}", state(&payout.state));
    if let Some(reason) = &payout.failure_reason {
        let _ = writeln!(out, "Failure reason: {}", escape_controls(reason));
    }
    time(
        out,
        "Resolved",
        &payout.resolved_at,
        &payout.resolved_at_unix,
    );
}

fn bonds_block(out: &mut String, bonds: &[BondRecord]) {
    let _ = writeln!(out, "\n🔐 Bonds\n{THIN_RULE}");
    for (index, bond) in bonds.iter().enumerate() {
        let _ = write!(
            out,
            "{}. {}  {} sats  {}  ({})",
            index + 1,
            escape_controls(&bond.role),
            bond.amount_sats,
            escape_controls(&bond.state),
            escape_controls(&bond.kind)
        );
        if let Some(reason) = &bond.slash_reason {
            let _ = write!(out, "  slashed: {}", escape_controls(reason));
        }
        if let Some(root) = &bond.root_order_id {
            let _ = write!(out, "  on root order {root}");
        }
        out.push('\n');
    }
}

/// A Lightning state, with what the two states that are not the Lightning
/// node's own answer mean.
fn state(state: &str) -> String {
    let note = match state {
        "unavailable" => " (the node could not ask its Lightning node)",
        "not-found" => " (its Lightning node has no record of it)",
        _ => "",
    };
    format!("{}{note}", escape_controls(state))
}

fn time(out: &mut String, label: &str, text: &Option<String>, unix: &Option<String>) {
    if let Some(line) = time_line(label, text.as_deref(), unix.as_deref()) {
        let _ = writeln!(out, "{line}");
    }
}

fn key(key: &Option<String>) -> &str {
    key.as_deref().unwrap_or("none")
}

/// Which side a pair of flags names; `None` when neither is set.
fn sides(buyer: bool, seller: bool) -> Option<&'static str> {
    match (buyer, seller) {
        (true, true) => Some("buyer and seller"),
        (true, false) => Some("buyer"),
        (false, true) => Some("seller"),
        (false, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::prelude::{
        BondInfo, BondKind, BondRole, BondSlashReason, BondState, CashuEscrowInfo, HoldInvoiceInfo,
        HoldInvoiceState, OrderInfo, PayoutFailureReason, PayoutInfo, PayoutState,
    };
    use nostr_sdk::prelude::Keys;
    use uuid::Uuid;

    const ORDER: Uuid = Uuid::from_u128(1);
    const ROOT: Uuid = Uuid::from_u128(2);

    fn pending() -> OrderInfo {
        OrderInfo {
            id: ORDER,
            kind: "sell".to_string(),
            status: "pending".to_string(),
            amount: 100_000,
            fee: 600,
            fiat_code: "EUR".to_string(),
            fiat_amount: 50,
            payment_method: "SEPA".to_string(),
            premium: 1,
            created_at: 1_792_108_800,
            expires_at: 1_792_195_200,
            ..Default::default()
        }
    }

    fn rendered(reply: &OrderInfo) -> String {
        let node = Keys::generate().public_key();
        render_admin_order_info(&AdminOrderInfoData::from_reply(&node, reply).0)
    }

    #[test]
    fn a_pending_order_prints_the_order_block_only() {
        let text = rendered(&pending());

        for needle in [
            "🔎 Order 00000000-0000-0000-0000-000000000001",
            "Kind: sell",
            "Status: pending",
            "Amount: 100000 sats",
            "Fee: 600 sats",
            "Fiat Code: EUR",
            "Fiat Amount: 50\n",
            "Payment Method: SEPA",
            "Premium: 1%",
            "Created: 2026-10-16 00:00:00 UTC",
            "Expires: 2026-10-17 00:00:00 UTC",
            "Buyer: none",
            "Seller: none",
            "Failed payment: no (attempts: 0)",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
        for absent in [
            "Escrow",
            "Payout",
            "Bonds",
            "Taken",
            "Hold settled",
            "Dispute",
            "cancel",
            "Range",
        ] {
            assert!(!text.contains(absent), "{absent:?} in:\n{text}");
        }
    }

    #[test]
    fn a_full_order_prints_the_four_blocks() {
        let buyer = Keys::generate().public_key().to_hex();
        let text = rendered(&OrderInfo {
            status: "settled-hold-invoice".to_string(),
            min_amount: Some(10),
            max_amount: Some(100),
            range_parent_id: Some(ROOT),
            taken_at: Some(1_792_108_900),
            buyer_trade_pubkey: Some(buyer.clone()),
            buyer_dispute: true,
            seller_dispute: true,
            seller_cooperativecancel: true,
            failed_payment: true,
            payment_attempts: 2,
            hold_settled_at: Some(1_792_109_050),
            hold_invoice: Some(HoldInvoiceInfo {
                state: HoldInvoiceState::Settled,
                held_at: Some(1_792_109_000),
            }),
            payout: Some(PayoutInfo {
                state: PayoutState::Failed,
                failure_reason: Some(PayoutFailureReason::NoRoute),
                resolved_at: Some(1_792_109_100),
            }),
            bonds: vec![
                BondInfo {
                    role: BondRole::Maker,
                    amount: 5_000,
                    state: BondState::Locked,
                    slash_reason: None,
                    kind: BondKind::Bond,
                    root_order_id: Some(ROOT),
                },
                BondInfo {
                    role: BondRole::Taker,
                    amount: 1_000,
                    state: BondState::PendingPayout,
                    slash_reason: Some(BondSlashReason::Timeout),
                    kind: BondKind::Bond,
                    root_order_id: None,
                },
            ],
            ..pending()
        });

        for needle in [
            "Status: settled-hold-invoice",
            "Fiat Amount: 50 (range 10-100)",
            "Range Parent: 00000000-0000-0000-0000-000000000002",
            "Taken: 2026-10-16 00:01:40 UTC",
            &format!("Buyer: {buyer}"),
            "Dispute opened by: buyer and seller",
            "Cooperative cancel asked by: seller",
            "Failed payment: yes (attempts: 2)",
            "Hold settled: 2026-10-16 00:04:10 UTC",
            "Escrow: hold invoice",
            "State: settled\n",
            "Held: 2026-10-16 00:03:20 UTC",
            "Payout to the buyer",
            "State: failed\n",
            "Failure reason: no-route",
            "Resolved: 2026-10-16 00:05:00 UTC",
            "Bonds",
            "1. maker  5000 sats  locked  (bond)  on root order 00000000-0000-0000-0000-000000000002",
            "2. taker  1000 sats  pending-payout  (bond)  slashed: timeout",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
    }

    /// A part the node could not ask about is said, not left out.
    #[test]
    fn unavailable_and_not_found_are_explained() {
        let text = rendered(&OrderInfo {
            hold_invoice: Some(HoldInvoiceInfo {
                state: HoldInvoiceState::Unavailable,
                held_at: None,
            }),
            payout: Some(PayoutInfo {
                state: PayoutState::NotFound,
                failure_reason: None,
                resolved_at: None,
            }),
            ..pending()
        });

        assert!(
            text.contains("State: unavailable (the node could not ask its Lightning node)"),
            "{text}"
        );
        assert!(
            text.contains("State: not-found (its Lightning node has no record of it)"),
            "{text}"
        );
        assert!(!text.contains("Held:") && !text.contains("Resolved:"));
    }

    #[test]
    fn a_cashu_order_prints_its_escrow() {
        let text = rendered(&OrderInfo {
            cashu_escrow: Some(CashuEscrowInfo {
                mint_url: "https://mint.example".to_string(),
                locked_at: Some(1_792_109_000),
            }),
            ..pending()
        });

        assert!(text.contains("Escrow: Cashu"), "{text}");
        assert!(text.contains("Mint: https://mint.example"), "{text}");
        assert!(text.contains("Locked: 2026-10-16 00:03:20 UTC"), "{text}");
        assert!(!text.contains("hold invoice"));
    }

    /// Everything the node wrote is escaped: no control character of a reply
    /// reaches the terminal.
    #[test]
    fn text_from_the_node_is_escaped() {
        let hostile = "x\u{1b}[31m\ny";
        let text = rendered(&OrderInfo {
            kind: hostile.to_string(),
            status: hostile.to_string(),
            fiat_code: hostile.to_string(),
            payment_method: hostile.to_string(),
            cashu_escrow: Some(CashuEscrowInfo {
                mint_url: hostile.to_string(),
                locked_at: None,
            }),
            ..pending()
        });

        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert_eq!(text.matches("x\\u{1b}[31m\\ny").count(), 5, "{text}");
        // Only the line ends this renderer wrote are left.
        assert!(text.lines().all(|line| !line.contains('\r')));
    }
}
