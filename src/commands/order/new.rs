//! `order.new`: publish one order to the node and keep what it created.
//!
//! One signed `new-order` goes out through `msg::exchange`, signed by a fresh
//! trade key under the identity seal. The trade index is reserved first, and
//! so recorded as spent before anything is published (see [`super::write`]):
//! the next order never reuses a key the node may have seen, even when this
//! run dies before the reply. The reply is mapped to a typed outcome and the
//! order row is written afterwards when the node created the order; a request
//! that never left and a refusal give the index back, except a refusal for a
//! stale index. The request is never retried. The command prints nothing and
//! never touches a client.

use chrono::Utc;
use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use super::write::{self, reserve_code, Sent};
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Exchanged, Expect, Lane, Outbound, PublicReadConfig};
use crate::outcome::new::{OrderNewData, OrderNewInput};
use crate::outcome::{Envelope, ErrorCode, Warnings};
use crate::store::{Role, Store};

pub const COMMAND: &str = "order.new";

pub type NewOutcome = Envelope<OrderNewData, ()>;

/// How the request is sent; shared by the write commands.
pub type NewOptions = WriteOptions;

/// Codes that leave the outcome `uncertain` when the request was published.
pub const UNCERTAIN_CODES: [ErrorCode; 6] = [
    ErrorCode::ReplyTimeout,
    ErrorCode::ReplyMalformed,
    ErrorCode::RequestRejected,
    ErrorCode::RelayReadFailed,
    ErrorCode::PowRequired,
    ErrorCode::OrderNotSaved,
];

/// Create `input` on the node, signing with the next trade key under
/// `identity`. The trade is reserved here, after every local check and the
/// confirmation, right before publishing; the outcome reports the index and
/// key it got, which is the previewed one unless another command reserved one
/// meanwhile.
pub async fn order_new(
    input: &OrderNewInput,
    config: &PublicReadConfig,
    identity: &Keys,
    store: &Store,
    options: NewOptions,
    limits: &ExchangeLimits,
) -> NewOutcome {
    let reservation = match store.reserve_trade().await {
        Ok(reservation) => reservation,
        Err(error) => return unsent(reserve_code(error), Warnings::new()),
    };
    let trade = &reservation.trade;
    let request_id = Uuid::new_v4().as_u128() as u64;
    let request = Message::new_order(
        None,
        Some(request_id),
        Some(trade.index),
        Action::NewOrder,
        Some(Payload::Order(input.small_order(Utc::now().timestamp()))),
    );
    let outbound = Outbound {
        identity,
        signer: &trade.keys,
        recipient: config.mostro_pubkey(),
        message: request,
        pow: options.pow,
        lane: Lane::FirstContact,
        check_protocol: options.check_protocol,
    };
    let expect = Expect {
        allowed_actions: vec![Action::NewOrder, Action::PayBondInvoice, Action::CantDo],
        by_order: None,
    };
    let sent = Sent {
        command: COMMAND,
        reservation: &reservation,
        store,
        request_id,
        role: Role::Maker,
    };
    match exchange(config, outbound, &expect, limits).await {
        Err(failure) => sent.failed(&failure).await,
        Ok(exchanged) => {
            let mut outcome = answer(&sent, config, &exchanged).await;
            outcome.add_warnings(exchanged.warnings());
            outcome
        }
    }
}

/// What the exchange gave back, mapped to an outcome and recorded.
async fn answer(sent: &Sent<'_>, config: &PublicReadConfig, exchanged: &Exchanged) -> NewOutcome {
    let kind = exchanged.reply().get_inner_message_kind();
    match (&kind.action, &kind.payload) {
        (Action::NewOrder, Some(Payload::Order(order))) if order.id.is_some() => {
            match OrderNewData::from_created(
                &config.mostro_pubkey(),
                order,
                sent.reservation.trade.index,
                &sent.reservation.trade.keys.public_key(),
            ) {
                Some((data, warnings)) => sent.saved(order, data, warnings).await,
                None => sent.malformed(),
            }
        }
        (Action::PayBondInvoice, Some(Payload::PaymentRequest(Some(order), invoice, _)))
            if order.id.is_some() =>
        {
            match OrderNewData::from_bond(
                &config.mostro_pubkey(),
                order,
                invoice,
                sent.reservation.trade.index,
                &sent.reservation.trade.keys.public_key(),
            ) {
                Some((data, warnings)) => sent.saved(order, data, warnings).await,
                None => sent.malformed(),
            }
        }
        (Action::CantDo, payload) => sent.refused(payload).await,
        _ => sent.malformed(),
    }
}

/// A failure before anything was published.
pub fn unsent(code: ErrorCode, warnings: Warnings) -> NewOutcome {
    write::unsent(COMMAND, code, warnings)
}
