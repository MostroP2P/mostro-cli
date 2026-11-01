//! `order.new`: publish one order to the node and keep what it created.
//!
//! One signed `new-order` goes out through `msg::exchange`, signed by a fresh
//! trade key under the identity seal. The reply is mapped to a typed outcome and
//! the local state is written afterwards: the order row and the trade index
//! together when the node created the order, the index alone whenever the
//! request was published and the node did not provably refuse it, so the next
//! order never reuses a key the node may have seen. The request is never
//! retried. The command prints nothing and never touches a client.

use chrono::Utc;
use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use super::write::{self, Sent};
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Exchanged, Expect, Outbound, PublicReadConfig};
use crate::outcome::new::{OrderNewData, OrderNewInput};
use crate::outcome::{Envelope, ErrorCode, Warnings};
use crate::store::{Store, Trade};

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

/// Create `input` on the node, signing with `trade` under `identity`.
pub async fn order_new(
    input: &OrderNewInput,
    config: &PublicReadConfig,
    identity: &Keys,
    trade: &Trade,
    store: &Store,
    options: NewOptions,
    limits: &ExchangeLimits,
) -> NewOutcome {
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
        check_protocol: options.check_protocol,
    };
    let expect = Expect {
        allowed_actions: vec![Action::NewOrder, Action::PayBondInvoice, Action::CantDo],
    };
    let sent = Sent {
        command: COMMAND,
        trade,
        store,
        request_id,
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
                sent.trade.index,
                &sent.trade.keys.public_key(),
            ) {
                Some((data, warnings)) => sent.saved(order, data, warnings).await,
                None => sent.malformed().await,
            }
        }
        (Action::PayBondInvoice, Some(Payload::PaymentRequest(Some(order), invoice, _)))
            if order.id.is_some() =>
        {
            match OrderNewData::from_bond(
                &config.mostro_pubkey(),
                order,
                invoice,
                sent.trade.index,
                &sent.trade.keys.public_key(),
            ) {
                Some((data, warnings)) => sent.saved(order, data, warnings).await,
                None => sent.malformed().await,
            }
        }
        (Action::CantDo, payload) => sent.refused(payload),
        _ => sent.malformed().await,
    }
}

/// A failure before anything was published.
pub fn unsent(code: ErrorCode, warnings: Warnings) -> NewOutcome {
    write::unsent(COMMAND, code, warnings)
}
