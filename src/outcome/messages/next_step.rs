//! The typed next step of a message record: what the message asked the user to
//! do when the node sent it. A consumer treats a value it does not know as null
//! (see the versioning rule in `docs/commands.md`).

use mostro_core::prelude::{Action, Kind, MessageKind, Payload};
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
    /// Send the fiat, then `order fiat-sent`. Sent to the buyer once the
    /// seller's hold invoice is paid; act only while the order is `active`.
    FiatSent,
    /// Check that the fiat is in the account, then `order release`. Only on
    /// the seller's copy of `fiat-sent-ok`, when the local order row says the
    /// user is the seller; act only while the order is `fiat-sent`.
    Release,
    /// Rate the counterparty: `order rate`. Sent to both sides when the trade
    /// is over.
    Rate,
}

/// The user's side of a trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeRole {
    Buyer,
    Seller,
}

impl TradeRole {
    /// The user's side in an order of `kind` that the user made (`made`) or
    /// took. The local row records both (`kind`, `is_mine`); rows written by
    /// 0.16.3 and earlier say "made" for every order.
    pub fn of_order(kind: Kind, made: bool) -> Self {
        match (kind, made) {
            (Kind::Sell, true) | (Kind::Buy, false) => Self::Seller,
            (Kind::Sell, false) | (Kind::Buy, true) => Self::Buyer,
        }
    }
}

impl NextStep {
    /// The step `message` asks for, if any, when the user's side in its order
    /// is `role` (`None`: not known). `fiat-sent-ok` asks the seller to
    /// release only on the copy that answers no request: the buyer's copy
    /// echoes the buyer's `fiat-sent` request id.
    pub fn of(message: &MessageKind, role: Option<TradeRole>) -> Option<Self> {
        match message.action {
            Action::FiatSentOk
                if role == Some(TradeRole::Seller) && message.request_id.is_none() =>
            {
                Some(Self::Release)
            }
            Action::PayBondInvoice => Some(Self::PayBondInvoice),
            Action::AddInvoice => Some(Self::AddInvoice),
            Action::PayInvoice => Some(Self::PayHoldInvoice),
            Action::AddBondInvoice => Some(Self::AddBondInvoice),
            Action::HoldInvoicePaymentAccepted => Some(Self::FiatSent),
            Action::Rate => Some(Self::Rate),
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

    const SIDES: [Option<TradeRole>; 3] = [None, Some(TradeRole::Buyer), Some(TradeRole::Seller)];

    fn asked(action: Action, request_id: Option<u64>) -> MessageKind {
        MessageKind::new(Some(Uuid::from_u128(1)), request_id, None, action, None)
    }

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
        let whatever_the_side = [
            (Action::PayBondInvoice, NextStep::PayBondInvoice),
            (Action::AddInvoice, NextStep::AddInvoice),
            (Action::PayInvoice, NextStep::PayHoldInvoice),
            (Action::AddBondInvoice, NextStep::AddBondInvoice),
            (Action::HoldInvoicePaymentAccepted, NextStep::FiatSent),
            (Action::Rate, NextStep::Rate),
        ];
        let quiet = [
            Action::WaitingSellerToPay,
            Action::CantDo,
            Action::BondInvoiceAccepted,
            Action::NewOrder,
            Action::Released,
            Action::BuyerTookOrder,
            Action::PurchaseCompleted,
            Action::HoldInvoicePaymentSettled,
            Action::RateReceived,
        ];
        for side in SIDES {
            for request in [Some(77), None] {
                for (action, step) in &whatever_the_side {
                    let message = asked(action.clone(), request);
                    assert_eq!(NextStep::of(&message, side), Some(*step), "{action:?}");
                }
                for action in &quiet {
                    let message = asked(action.clone(), request);
                    assert_eq!(NextStep::of(&message, side), None, "{action:?}");
                }
                let release = (side == Some(TradeRole::Seller) && request.is_none())
                    .then_some(NextStep::Release);
                let fiat_sent_ok = asked(Action::FiatSentOk, request);
                assert_eq!(
                    NextStep::of(&fiat_sent_ok, side),
                    release,
                    "{side:?} {request:?}"
                );
            }
        }
    }

    #[test]
    fn the_side_follows_the_order_kind_and_who_made_it() {
        assert_eq!(TradeRole::of_order(Kind::Sell, true), TradeRole::Seller);
        assert_eq!(TradeRole::of_order(Kind::Buy, false), TradeRole::Seller);
        assert_eq!(TradeRole::of_order(Kind::Sell, false), TradeRole::Buyer);
        assert_eq!(TradeRole::of_order(Kind::Buy, true), TradeRole::Buyer);
    }

    #[test]
    fn a_failed_payment_asks_for_an_invoice_only_with_no_attempt_left() {
        for side in SIDES {
            assert_eq!(NextStep::of(&failed(2), side), None, "the node retries");
            assert_eq!(NextStep::of(&failed(1), side), None, "retries once");
            assert_eq!(NextStep::of(&failed(0), side), Some(NextStep::AddInvoice));
            let bare = kind(Action::PaymentFailed, None);
            assert_eq!(NextStep::of(&bare, side), None, "no payload, no count");
        }
    }
}
