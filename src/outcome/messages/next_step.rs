//! The typed next step of a message record: what the message asked the user to
//! do when the node sent it. A consumer treats a value it does not know as null
//! (see the versioning rule in `docs/commands.md`).

use mostro_core::prelude::{Action, MessageKind, Payload};
use schemars::JsonSchema;
use serde::Serialize;

/// What the node asks the user to do next, for the messages that ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NextStep {
    /// Pay the bond invoice in the payload.
    PayBondInvoice,
    /// Send an invoice for the order: `order add-invoice`. Also after a failed
    /// payout that the node will not retry.
    AddInvoice,
    /// Pay the hold invoice in the payload.
    PayHoldInvoice,
    /// Send a payout invoice for a slashed bond: `bond add-invoice`.
    AddBondInvoice,
}

impl NextStep {
    /// The step `message` asks for, if any.
    pub fn of(message: &MessageKind) -> Option<Self> {
        match message.action {
            Action::PayBondInvoice => Some(Self::PayBondInvoice),
            Action::AddInvoice => Some(Self::AddInvoice),
            Action::PayInvoice => Some(Self::PayHoldInvoice),
            Action::AddBondInvoice => Some(Self::AddBondInvoice),
            Action::PaymentFailed => match &message.payload {
                Some(Payload::PaymentFailed(info)) if info.payment_attempts == 0 => {
                    Some(Self::AddInvoice)
                }
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mostro_core::prelude::PaymentFailedInfo;
    use uuid::Uuid;

    fn kind(action: Action, payload: Option<Payload>) -> MessageKind {
        MessageKind::new(Some(Uuid::from_u128(1)), Some(77), None, action, payload)
    }

    fn failed(attempts: u32) -> MessageKind {
        kind(
            Action::PaymentFailed,
            Some(Payload::PaymentFailed(PaymentFailedInfo {
                payment_attempts: attempts,
                payment_retries_interval: 60,
            })),
        )
    }

    #[test]
    fn the_actions_that_ask_something_have_a_next_step() {
        for (action, step) in [
            (Action::PayBondInvoice, NextStep::PayBondInvoice),
            (Action::AddInvoice, NextStep::AddInvoice),
            (Action::PayInvoice, NextStep::PayHoldInvoice),
            (Action::AddBondInvoice, NextStep::AddBondInvoice),
        ] {
            assert_eq!(NextStep::of(&kind(action.clone(), None)), Some(step));
        }
        for quiet in [
            Action::WaitingSellerToPay,
            Action::CantDo,
            Action::HoldInvoicePaymentAccepted,
            Action::BondInvoiceAccepted,
            Action::NewOrder,
            Action::Released,
        ] {
            assert_eq!(NextStep::of(&kind(quiet.clone(), None)), None, "{quiet:?}");
        }
    }

    #[test]
    fn a_failed_payment_asks_for_an_invoice_only_with_no_attempt_left() {
        assert_eq!(NextStep::of(&failed(2)), None, "the node retries");
        assert_eq!(NextStep::of(&failed(1)), None, "the node retries once");
        assert_eq!(NextStep::of(&failed(0)), Some(NextStep::AddInvoice));
        assert_eq!(
            NextStep::of(&kind(Action::PaymentFailed, None)),
            None,
            "no payload, no count"
        );
    }
}
