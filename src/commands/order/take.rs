//! `order.take-sell` and `order.take-buy`: take an order from the book.
//!
//! One signed take goes out through `msg::exchange`, signed by a fresh trade
//! key under the identity seal, and the reply is mapped to what the node asked
//! the taker to do next. The local state is written afterwards by the same
//! rules as `order.new` (see [`super::write`]): the order and the trade index
//! together when the node accepted, the index alone whenever the take was
//! published and not refused. The take is never retried. The command prints
//! nothing and never touches a client.

use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

pub use super::new::UNCERTAIN_CODES;
use super::write::{self, Sent};
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Exchanged, Expect, Outbound, PublicReadConfig};
use crate::outcome::take::{OrderTakeData, OrderTakeInput, TakeSide, Taken};
use crate::outcome::{Envelope, ErrorCode, Warnings};
use crate::store::{Store, Trade};

pub type TakeOutcome = Envelope<OrderTakeData, ()>;

/// Take `input` from the node, signing with `trade` under `identity`.
pub async fn order_take(
    input: &OrderTakeInput,
    config: &PublicReadConfig,
    identity: &Keys,
    trade: &Trade,
    store: &Store,
    options: WriteOptions,
    limits: &ExchangeLimits,
) -> TakeOutcome {
    let request_id = Uuid::new_v4().as_u128() as u64;
    let request = Message::new_order(
        Some(input.order()),
        Some(request_id),
        Some(trade.index),
        input.side().action(),
        input.payload(),
    );
    let outbound = Outbound {
        identity,
        signer: &trade.keys,
        recipient: config.mostro_pubkey(),
        message: request,
        pow: options.pow,
        check_protocol: options.check_protocol,
    };
    let expect = Expect {
        allowed_actions: vec![
            Action::AddInvoice,
            Action::WaitingSellerToPay,
            Action::PayInvoice,
            Action::PayBondInvoice,
            Action::CantDo,
        ],
        by_order: None,
    };
    let sent = Sent {
        command: input.side().command(),
        trade,
        store,
        request_id,
    };
    match exchange(config, outbound, &expect, limits).await {
        Err(failure) => sent.failed(&failure).await,
        Ok(exchanged) => {
            let mut outcome = answer(&sent, input, config, &exchanged).await;
            outcome.add_warnings(exchanged.warnings());
            outcome
        }
    }
}

/// What the exchange gave back, mapped to an outcome and recorded.
async fn answer(
    sent: &Sent<'_>,
    input: &OrderTakeInput,
    config: &PublicReadConfig,
    exchanged: &Exchanged,
) -> TakeOutcome {
    let kind = exchanged.reply().get_inner_message_kind();
    let taken = match (input.side(), &kind.action, &kind.payload) {
        (_, Action::CantDo, payload) => return sent.refused(payload),
        (_, Action::PayBondInvoice, Some(Payload::PaymentRequest(Some(order), invoice, _))) => {
            Taken::PayBond(order.clone(), invoice.clone())
        }
        (TakeSide::Sell, Action::AddInvoice, Some(Payload::Order(order))) => {
            Taken::AddInvoice(order.clone())
        }
        (TakeSide::Sell, Action::WaitingSellerToPay, None) if kind.id == Some(input.order()) => {
            Taken::WaitingSellerToPay
        }
        (
            TakeSide::Buy,
            Action::PayInvoice,
            Some(Payload::PaymentRequest(Some(order), invoice, _)),
        ) => Taken::PayInvoice(order.clone(), invoice.clone()),
        _ => return sent.malformed().await,
    };
    match OrderTakeData::from_taken(
        &config.mostro_pubkey(),
        input,
        &taken,
        sent.trade.index,
        &sent.trade.keys.public_key(),
    ) {
        Some((data, order, warnings)) => sent.saved(&order, data, warnings).await,
        None => sent.malformed().await,
    }
}

/// A failure before anything was published.
pub fn unsent(side: TakeSide, code: ErrorCode, warnings: Warnings) -> TakeOutcome {
    write::unsent(side.command(), code, warnings)
}
