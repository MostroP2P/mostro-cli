//! `order.add-invoice` and `bond.add-invoice`: answer the node's request for
//! an invoice on an order the user already holds.
//!
//! One signed reply goes out through `msg::exchange`, signed by the order's
//! stored trade key under the identity seal, with the order id and no trade
//! index: these commands never spend an index, so a failed run leaves the
//! database as it was. The reply is mapped to a typed outcome. After an
//! accepted `order.add-invoice` the local status follows the node's; a failed
//! update is a warning, not a failure, because the node already holds the
//! invoice. The request is never retried. The commands print nothing and never
//! touch a client.

use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use super::write::{reason, unsent};
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Exchanged, Expect, Outbound, PublicReadConfig};
use crate::outcome::invoice::{
    Answered, BondInvoiceData, InvoiceInput, InvoiceKind, OrderInvoiceData,
};
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, WarningCode,
    Warnings,
};
use crate::store::{Store, StoreError};

pub type OrderInvoiceOutcome = Envelope<OrderInvoiceData, ()>;
pub type BondInvoiceOutcome = Envelope<BondInvoiceData, ()>;

/// Send the buyer's payout invoice for the order of `input`.
pub async fn order_add_invoice(
    input: &InvoiceInput,
    config: &PublicReadConfig,
    identity: &Keys,
    store: &Store,
    options: WriteOptions,
    limits: &ExchangeLimits,
) -> OrderInvoiceOutcome {
    let command = InvoiceKind::Order.command();
    let (trade, exchanged) = match send(input, config, identity, store, options, limits).await {
        Ok(sent) => sent,
        Err(outcome) => return outcome,
    };
    let kind = exchanged.reply().get_inner_message_kind();
    let answered = match (&kind.action, &kind.payload) {
        (Action::CantDo, payload) => return refused(command, payload),
        (Action::WaitingSellerToPay, None) if kind.id == Some(input.order()) => {
            Answered::WaitingSellerToPay
        }
        (Action::HoldInvoicePaymentAccepted, Some(Payload::Order(order))) => {
            Answered::PaymentAccepted(Box::new(order.clone()))
        }
        (Action::InvoiceUpdated, None) => Answered::InvoiceUpdated,
        _ => return malformed(command),
    };
    let Some((data, status, mut warnings)) = OrderInvoiceData::from_answer(
        &config.mostro_pubkey(),
        input,
        &answered,
        &trade.public_key(),
    ) else {
        return malformed(command);
    };
    let persistence = match status {
        None => Persistence::NotAttempted,
        Some(status) => match store.set_status(input.order(), status).await {
            Ok(()) => Persistence::Applied,
            Err(_) => {
                warnings.add(WarningCode::LocalStatusStale);
                Persistence::Failed
            }
        },
    };
    warnings.merge(exchanged.warnings());
    Envelope::ok(command, data, warnings, accepted(persistence))
}

/// Send the payout invoice for the user's share of a slashed bond.
pub async fn bond_add_invoice(
    input: &InvoiceInput,
    config: &PublicReadConfig,
    identity: &Keys,
    store: &Store,
    options: WriteOptions,
    limits: &ExchangeLimits,
) -> BondInvoiceOutcome {
    let command = InvoiceKind::Bond.command();
    let (trade, exchanged) = match send(input, config, identity, store, options, limits).await {
        Ok(sent) => sent,
        Err(outcome) => return outcome,
    };
    let kind = exchanged.reply().get_inner_message_kind();
    let data = match (&kind.action, &kind.payload) {
        (Action::CantDo, payload) => return refused(command, payload),
        (Action::BondInvoiceAccepted, Some(Payload::Order(order))) => {
            BondInvoiceData::from_ack(&config.mostro_pubkey(), input, order, &trade.public_key())
        }
        _ => None,
    };
    match data {
        Some(data) => Envelope::ok(
            command,
            data,
            exchanged.warnings().clone(),
            accepted(Persistence::NotAttempted),
        ),
        None => malformed(command),
    }
}

/// Look up the order's key, publish the invoice and wait for the answer. The
/// error side is the finished outcome of everything that went wrong first.
async fn send<D>(
    input: &InvoiceInput,
    config: &PublicReadConfig,
    identity: &Keys,
    store: &Store,
    options: WriteOptions,
    limits: &ExchangeLimits,
) -> Result<(Keys, Exchanged), Envelope<D, ()>> {
    let kind = input.kind();
    let command = kind.command();
    let trade = match store.order_keys(input.order()).await {
        Ok(Some(keys)) => keys,
        Ok(None) | Err(StoreError::Missing) => {
            return Err(unsent(command, ErrorCode::OrderUnknown, Warnings::new()))
        }
        Err(StoreError::Unwritable) => {
            return Err(unsent(command, ErrorCode::StoreUnwritable, Warnings::new()))
        }
    };
    let request_id = Uuid::new_v4().as_u128() as u64;
    let outbound = Outbound {
        identity,
        signer: &trade,
        recipient: config.mostro_pubkey(),
        message: Message::new_order(
            Some(input.order()),
            Some(request_id),
            None,
            kind.action(),
            Some(input.payload()),
        ),
        pow: options.pow,
        check_protocol: options.check_protocol,
    };
    let expect = match kind {
        InvoiceKind::Order => Expect {
            allowed_actions: vec![
                Action::WaitingSellerToPay,
                Action::HoldInvoicePaymentAccepted,
                Action::InvoiceUpdated,
                Action::CantDo,
            ],
            by_order: None,
        },
        // The ack carries no request id: it is matched on the order.
        InvoiceKind::Bond => Expect {
            allowed_actions: vec![Action::BondInvoiceAccepted, Action::CantDo],
            by_order: Some((input.order(), vec![Action::BondInvoiceAccepted])),
        },
    };
    match exchange(config, outbound, &expect, limits).await {
        Ok(exchanged) => Ok((trade, exchanged)),
        Err(failure) if failure.publication() == Publication::NotAttempted => {
            Err(unsent(command, failure.code(), failure.warnings().clone()))
        }
        // Published, or possibly published: the node may hold the invoice.
        Err(failure) => Err(Envelope::uncertain(
            command,
            failure.code(),
            None,
            failure.warnings().clone(),
            ExecutionMeta::write(
                failure.publication(),
                failure.daemon_acceptance(),
                Persistence::NotAttempted,
            ),
        )),
    }
}

fn accepted(persistence: Persistence) -> ExecutionMeta<()> {
    ExecutionMeta::write(
        Publication::Published,
        DaemonAcceptance::Accepted,
        persistence,
    )
}

/// The node refused: nothing was recorded, nothing changes locally.
fn refused<D>(command: &'static str, payload: &Option<Payload>) -> Envelope<D, ()> {
    Envelope::refused(
        Some(command),
        &reason(payload),
        Warnings::new(),
        ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Rejected,
            Persistence::NotAttempted,
        ),
    )
}

/// The node answered with something unusable: it may hold the invoice.
fn malformed<D>(command: &'static str) -> Envelope<D, ()> {
    Envelope::uncertain(
        command,
        ErrorCode::ReplyMalformed,
        None,
        Warnings::new(),
        ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Unknown,
            Persistence::NotAttempted,
        ),
    )
}
