//! Human presentation of `order fiat-sent`, `order release` and `order cancel`:
//! the preview shown before the request is sent and the result after it. Built
//! from the typed input and outcome only. Nothing from the node is shown beyond
//! the typed result.

use std::fmt::Write;

use crate::outcome::step::{StepData, StepInput, StepKind, StepResult};

const RULE: &str = "═══════════════════════════════════════";

/// What is about to be sent.
pub fn render_step_preview(input: &StepInput, trade_pubkey: &str) -> String {
    let mut out = String::new();
    let (title, warning) = match input.kind() {
        StepKind::FiatSent => (
            "💸 Fiat Sent Preview",
            "You tell the node that you paid the fiat.",
        ),
        StepKind::Release => (
            "🔓 Release Preview",
            "This settles the hold invoice and pays the buyer. It cannot be undone.",
        ),
        StepKind::Cancel => (
            "🛑 Cancel Preview",
            "The node decides if this cancels the order or asks the other party to agree.",
        ),
    };
    let _ = writeln!(out, "{title}\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", input.order());
    let _ = writeln!(out, "🔑 Trade Key: {trade_pubkey}");
    let _ = writeln!(out, "⚠️  {warning}");
    out
}

/// What the node did with the step.
pub fn render_step(data: &StepData) -> String {
    let (title, next) = match data.result() {
        StepResult::FiatSent => (
            "✅ Fiat Marked As Sent",
            "⏳ Wait for the seller to release the sats.",
        ),
        StepResult::Released => (
            "🎉 Sats Released",
            "✅ The hold invoice is settled and the buyer is being paid.",
        ),
        StepResult::Canceled => ("🛑 Order Canceled", "✅ The order is canceled."),
        StepResult::CancelRequested => (
            "⏳ Cancel Requested",
            "💡 The other party must confirm with `order cancel --order <id>` for the order to be canceled.",
        ),
        StepResult::CancelAccepted => (
            "🛑 Order Canceled By Both Parties",
            "✅ The cooperative cancel is complete.",
        ),
    };
    let mut out = String::new();
    let _ = writeln!(out, "{title}\n{RULE}");
    let _ = writeln!(out, "📋 Order ID: {}", data.order_id());
    let _ = writeln!(out, "🔑 Trade Key: {}", data.trade_pubkey());
    let _ = writeln!(out, "{next}");
    match (data.result(), data.next_trade_index()) {
        (StepResult::Released, Some(_)) => {
            let _ = writeln!(
                out,
                "💡 The rest of the range continues as a new order: run `order sync` to add it."
            );
        }
        (StepResult::FiatSent, Some(_)) => {
            let _ = writeln!(
                out,
                "💡 The rest of the range continues when the seller releases: run `order sync` then."
            );
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::prelude::Action;
    use nostr_sdk::prelude::Keys;
    use uuid::Uuid;

    fn data(kind: StepKind, action: Action) -> StepData {
        let (node, trade) = (Keys::generate().public_key(), Keys::generate().public_key());
        StepData::from_reply(
            &node,
            &StepInput::new(kind, Uuid::from_u128(9)),
            (&action, &None, Some(Uuid::from_u128(9))),
            &trade,
            None,
        )
        .unwrap()
        .0
    }

    #[test]
    fn each_result_has_its_own_title_and_names_the_order_and_trade_key() {
        let cases = [
            (
                StepKind::FiatSent,
                Action::FiatSentOk,
                "Fiat Marked As Sent",
            ),
            (
                StepKind::Release,
                Action::HoldInvoicePaymentSettled,
                "Sats Released",
            ),
            (StepKind::Cancel, Action::Canceled, "Order Canceled"),
            (
                StepKind::Cancel,
                Action::CooperativeCancelInitiatedByYou,
                "Cancel Requested",
            ),
            (
                StepKind::Cancel,
                Action::CooperativeCancelAccepted,
                "Canceled By Both Parties",
            ),
        ];
        for (kind, action, title) in cases {
            let data = data(kind, action);
            let text = render_step(&data);
            assert!(text.contains(title), "{title}: {text}");
            assert!(text.contains(&Uuid::from_u128(9).to_string()));
            assert!(text.contains(data.trade_pubkey()));
        }
    }

    #[test]
    fn a_step_that_sent_a_key_points_at_order_sync() {
        let node = Keys::generate().public_key();
        let trade = Keys::generate().public_key();
        let human = |kind, action: Action| {
            let data = StepData::from_reply(
                &node,
                &StepInput::new(kind, Uuid::from_u128(9)),
                (&action, &None, Some(Uuid::from_u128(9))),
                &trade,
                Some(4),
            )
            .unwrap()
            .0;
            render_step(&data)
        };
        assert!(human(StepKind::Release, Action::HoldInvoicePaymentSettled).contains("order sync"));
        assert!(human(StepKind::FiatSent, Action::FiatSentOk).contains("when the seller releases"));
        let plain = render_step(&data(StepKind::Release, Action::HoldInvoicePaymentSettled));
        assert!(!plain.contains("order sync"), "{plain}");
    }

    #[test]
    fn a_cancel_request_names_the_command_the_other_party_runs() {
        let text = render_step(&data(
            StepKind::Cancel,
            Action::CooperativeCancelInitiatedByYou,
        ));
        assert!(text.contains("order cancel --order"), "{text}");
    }

    #[test]
    fn the_release_preview_says_it_cannot_be_undone_and_shows_no_secret() {
        let input = StepInput::new(StepKind::Release, Uuid::from_u128(9));
        let text = render_step_preview(&input, "abc");
        assert!(text.contains("cannot be undone"), "{text}");
        assert!(text.contains("Trade Key: abc"));
        for kind in [StepKind::FiatSent, StepKind::Cancel] {
            let text = render_step_preview(&StepInput::new(kind, Uuid::from_u128(9)), "abc");
            assert!(!text.contains("cannot be undone"), "{text}");
        }
    }
}
