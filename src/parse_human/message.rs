//! Human presentation of `message.list`: one block per message with the next
//! step in words. An invoice the node sent is alone on its line so it can be
//! copied. Built from the typed outcome only; text that came from the node is
//! escaped for the terminal.

use std::fmt::Write;

use super::table::escape_controls;
use crate::outcome::info::OrderRecord;
use crate::outcome::messages::{MessageListData, MessagePayload, MessageRecord, NextStep};

const RULE: &str = "═══════════════════════════════════════";
const THIN_RULE: &str = "─────────────────────────────────────";

/// The messages the node sent, oldest first.
pub fn render_messages(data: &MessageListData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "📨 Messages from Mostro\n{RULE}");
    let _ = writeln!(
        out,
        "⏰ Last {} minutes, {} trade key(s) read",
        data.since_minutes(),
        data.keys_read()
    );
    if data.keys_not_read() != "0" {
        let _ = writeln!(
            out,
            "⚠️  {} older trade key(s) not read",
            data.keys_not_read()
        );
    }
    if data.messages().is_empty() {
        let _ = writeln!(out, "📭 No messages");
        return out;
    }
    let _ = writeln!(out, "📊 {} message(s):\n", data.messages().len());
    for (index, message) in data.messages().iter().enumerate() {
        block(&mut out, index + 1, message);
    }
    if data.truncated() {
        let _ = writeln!(
            out,
            "⚠️  There may be more messages than shown; narrow the window with --since."
        );
    }
    out
}

pub(crate) fn block(out: &mut String, number: usize, message: &MessageRecord) {
    let _ = writeln!(out, "📄 Message {number}:\n{THIN_RULE}");
    match message.created_at() {
        Some(at) => {
            let _ = writeln!(
                out,
                "📅 Time: {}",
                at.replace('T', " ").replace('Z', " UTC")
            );
        }
        None => {
            let _ = writeln!(out, "📅 Time: Invalid date");
        }
    }
    let _ = writeln!(out, "🎬 Action: {}", message.action());
    if let Some(order) = message.order_id() {
        let _ = writeln!(out, "📋 Order ID: {order}");
    }
    if let Some(index) = message.trade_index() {
        let _ = writeln!(out, "🔢 Trade Index: {index}");
    }
    payload(out, message.payload());
    if let Some(step) = message.next_step() {
        let _ = writeln!(out, "👉 Next: {}", next_step(step, message.order_id()));
    }
    out.push('\n');
}

/// The next step in words, with the record's order id (a UUID, never node
/// text) filled in, or `<id>` when the record names no order.
fn next_step(step: NextStep, order: Option<&str>) -> String {
    let id = order.unwrap_or("<id>");
    match step {
        NextStep::PayBondInvoice => "pay the bond invoice above with any Lightning wallet".into(),
        NextStep::AddInvoice => format!(
            "send an invoice for the sats: `order add-invoice --order {id} --invoice <invoice>`"
        ),
        NextStep::PayHoldInvoice => "pay the hold invoice above with any Lightning wallet".into(),
        NextStep::AddBondInvoice => format!(
            "send an invoice for your share: `bond add-invoice --order {id} --invoice <bolt11>`"
        ),
        NextStep::FiatSent => format!("send the fiat, then: `order fiat-sent --order {id}`"),
        NextStep::Rate => {
            format!("rate your counterparty: `order rate --order {id} --rating <1-5>`")
        }
    }
}

/// What the node will do after a failed payout: retry `attempts` more times
/// every `interval` seconds, or nothing at "0".
fn retry_line(attempts: &str, interval: &str) -> String {
    if attempts == "0" {
        return "🔁 Payment failed: the node will not retry the payment".to_string();
    }
    let times = if attempts == "1" { "time" } else { "times" };
    let seconds = if interval == "1" { "second" } else { "seconds" };
    format!(
        "🔁 Payment failed: the node will retry the payment up to {attempts} more {times}, every {interval} {seconds}"
    )
}

fn payload(out: &mut String, payload: &MessagePayload) {
    match payload {
        MessagePayload::None => {}
        MessagePayload::Order { order } => order_lines(out, order),
        MessagePayload::PaymentRequest {
            order,
            invoice,
            amount,
        } => {
            if let Some(order) = order {
                order_lines(out, order);
            }
            if let Some(amount) = amount {
                let _ = writeln!(out, "💰 Amount: {amount} sats");
            }
            let _ = writeln!(out, "🧾 Invoice:\n{}", escape_controls(invoice));
        }
        MessagePayload::BondPayoutRequest {
            order, slashed_at, ..
        } => {
            order_lines(out, order);
            if let Some(at) = slashed_at {
                let _ = writeln!(
                    out,
                    "⚖️  Slashed: {}",
                    at.replace('T', " ").replace('Z', " UTC")
                );
            }
        }
        MessagePayload::CantDo { reason } => {
            let _ = writeln!(
                out,
                "🚫 Refused: {}",
                reason.as_deref().unwrap_or("no reason given")
            );
        }
        MessagePayload::Peer {
            counterparty_pubkey,
        } => {
            let _ = writeln!(
                out,
                "👤 Counterparty: {}",
                escape_controls(counterparty_pubkey)
            );
        }
        MessagePayload::Text { text } => {
            let _ = writeln!(out, "💬 Text: {}", escape_controls(text));
        }
        MessagePayload::Amount { amount } => {
            let _ = writeln!(out, "💰 Amount: {amount} sats");
        }
        MessagePayload::PaymentFailed {
            payment_attempts,
            payment_retries_interval,
        } => {
            let _ = writeln!(
                out,
                "{}",
                retry_line(payment_attempts, payment_retries_interval)
            );
        }
        MessagePayload::Other { variant } => {
            let _ = writeln!(out, "📦 Payload: {}", escape_controls(variant));
        }
    }
}

fn order_lines(out: &mut String, order: &OrderRecord) {
    let status = order
        .status()
        .and_then(|status| serde_json::to_value(status).ok())
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string());
    let _ = writeln!(out, "📊 Status: {status}");
    let _ = writeln!(out, "💰 Order Amount: {} sats", order.sats_amount());
    let _ = writeln!(
        out,
        "💱 Fiat: {} {}",
        order.fiat_amount(),
        escape_controls(order.fiat_currency())
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::messages::MessageListInput;
    use crate::outcome::Warnings;
    use mostro_core::order::{Kind, SmallOrder, Status};
    use mostro_core::prelude::{Action, CantDoReason, Message, Payload, PaymentFailedInfo};
    use nostr_sdk::prelude::{EventId, Keys};
    use uuid::Uuid;

    const AT: u64 = 1_792_108_800;

    fn order() -> SmallOrder {
        SmallOrder::new(
            Some(Uuid::from_u128(1)),
            Some(Kind::Sell),
            Some(Status::Pending),
            6_233,
            "USD".to_string(),
            None,
            None,
            5,
            "bank".to_string(),
            1,
            None,
            None,
            None,
            Some(1_792_108_800),
            Some(1_792_195_200),
        )
    }

    fn record(action: Action, payload: Option<Payload>) -> MessageRecord {
        record_of(Some(Uuid::from_u128(1)), action, payload)
    }

    fn record_of(order: Option<Uuid>, action: Action, payload: Option<Payload>) -> MessageRecord {
        let message = Message::new_order(order, Some(77), None, action, payload);
        MessageRecord::from_message(
            &EventId::from_byte_array([7; 32]),
            AT,
            &message,
            Some(3),
            &Keys::generate().public_key(),
            &mut Warnings::new(),
        )
    }

    fn data(messages: Vec<MessageRecord>, truncated: bool, not_read: usize) -> MessageListData {
        let input = MessageListInput::validate(Some(60), None).unwrap();
        MessageListData::new(
            &Keys::generate().public_key(),
            &input,
            messages,
            truncated,
            (4, not_read),
        )
    }

    #[test]
    fn an_empty_window_says_so() {
        let text = render_messages(&data(Vec::new(), false, 0));
        assert!(text.contains("Last 60 minutes, 4 trade key(s) read"));
        assert!(text.contains("No messages"));
        assert!(!text.contains("not read"));
    }

    #[test]
    fn a_bond_invoice_is_alone_on_its_line_with_the_next_step_in_words() {
        let invoice = "lnbcrt1bondinvoice";
        let payload = Payload::PaymentRequest(Some(order()), invoice.into(), None);
        let text = render_messages(&data(
            vec![record(Action::PayBondInvoice, Some(payload))],
            false,
            0,
        ));
        assert!(text.lines().any(|line| line == invoice), "copyable: {text}");
        assert!(text.contains("Action: pay-bond-invoice"));
        assert!(text.contains("Order ID: 00000000-0000-0000-0000-000000000001"));
        assert!(text.contains("Trade Index: 3"));
        assert!(text.contains("Next: pay the bond invoice"));
    }

    #[test]
    fn the_next_step_names_the_command_with_the_order_filled_in() {
        let add = render_messages(&data(
            vec![record(Action::AddInvoice, Some(Payload::Order(order())))],
            false,
            0,
        ));
        assert!(
            add.contains("`order add-invoice --order 00000000-0000-0000-0000-000000000001 --invoice <invoice>`"),
            "{add}"
        );
        let bond = render_messages(&data(
            vec![record_of(
                Some(Uuid::from_u128(9)),
                Action::AddBondInvoice,
                None,
            )],
            false,
            0,
        ));
        assert!(
            bond.contains("`bond add-invoice --order 00000000-0000-0000-0000-000000000009 --invoice <bolt11>`"),
            "{bond}"
        );
        let anonymous = render_messages(&data(
            vec![record_of(None, Action::AddInvoice, None)],
            false,
            0,
        ));
        assert!(
            anonymous.contains("`order add-invoice --order <id> --invoice <invoice>`"),
            "no order id, a placeholder: {anonymous}"
        );
        let informing = render_messages(&data(
            vec![record(Action::WaitingSellerToPay, None)],
            false,
            0,
        ));
        assert!(!informing.contains("Next:"));
    }

    #[test]
    fn the_steps_of_a_trade_name_their_command_with_the_order_id() {
        let text = render_messages(&data(
            vec![
                record(Action::HoldInvoicePaymentAccepted, None),
                record(Action::FiatSentOk, None),
                record(Action::Rate, None),
            ],
            false,
            0,
        ));
        assert!(
            text.contains(
                "👉 Next: send the fiat, then: `order fiat-sent --order 00000000-0000-0000-0000-000000000001`\n"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "👉 Next: rate your counterparty: `order rate --order 00000000-0000-0000-0000-000000000001 --rating <1-5>`\n"
            ),
            "{text}"
        );
        assert_eq!(
            text.matches("Next:").count(),
            2,
            "fiat-sent-ok asks nothing: {text}"
        );
    }

    #[test]
    fn a_failed_payment_says_how_many_more_times_and_how_often_the_node_retries() {
        let failed = |attempts, interval| {
            let payload = Payload::PaymentFailed(PaymentFailedInfo {
                payment_attempts: attempts,
                payment_retries_interval: interval,
            });
            render_messages(&data(
                vec![record(Action::PaymentFailed, Some(payload))],
                false,
                0,
            ))
        };
        let retried = failed(2, 60);
        assert!(
            retried.contains(
                "🔁 Payment failed: the node will retry the payment up to 2 more times, every 60 seconds\n"
            ),
            "{retried}"
        );
        assert!(!retried.contains("Next:"), "the node retries: {retried}");
        let once = failed(1, 1);
        assert!(
            once.contains("up to 1 more time, every 1 second\n"),
            "{once}"
        );
        assert!(!once.contains("Next:"), "{once}");
        let given_up = failed(0, 60);
        assert!(
            given_up.contains("🔁 Payment failed: the node will not retry the payment\n"),
            "{given_up}"
        );
        assert!(
            given_up.contains(
                "👉 Next: send an invoice for the sats: `order add-invoice --order 00000000-0000-0000-0000-000000000001 --invoice <invoice>`"
            ),
            "{given_up}"
        );
    }

    #[test]
    fn text_from_the_node_cannot_drive_the_terminal() {
        let payload = Payload::TextMessage("hi\u{1b}[31m red".into());
        let text = render_messages(&data(
            vec![record(Action::HoldInvoicePaymentAccepted, Some(payload))],
            false,
            0,
        ));
        assert!(!text.contains('\u{1b}'), "{text:?}");
    }

    #[test]
    fn a_refusal_names_its_reason_and_a_peer_key_is_shown_as_data() {
        let refusal = render_messages(&data(
            vec![record(
                Action::CantDo,
                Some(Payload::CantDo(Some(CantDoReason::InvalidPeer))),
            )],
            false,
            0,
        ));
        assert!(refusal.contains("Refused: invalid_peer"), "{refusal}");
    }

    #[test]
    fn limits_are_stated_when_they_applied() {
        let text = render_messages(&data(
            vec![record(Action::WaitingSellerToPay, None)],
            true,
            7,
        ));
        assert!(text.contains("7 older trade key(s) not read"));
        assert!(text.contains("There may be more messages than shown"));
        assert!(!render_messages(&data(Vec::new(), false, 0)).contains("There may be more"));
    }
}
