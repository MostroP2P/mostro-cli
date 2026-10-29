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
use mostro_core::order::SmallOrder;
use mostro_core::prelude::{Action, CantDoReason, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use crate::msg::{
    exchange, ExchangeFailure, ExchangeLimits, Exchanged, Expect, Outbound, PublicReadConfig,
};
use crate::outcome::new::{OrderNewData, OrderNewInput};
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, Warnings,
};
use crate::store::{Store, Trade};

pub const COMMAND: &str = "order.new";

pub type NewOutcome = Envelope<OrderNewData, ()>;

/// How the request is sent: the work to mine and whether to check the node's
/// protocol (off when the user chose the transport explicitly).
#[derive(Clone, Copy, Debug)]
pub struct NewOptions {
    pub pow: u8,
    pub check_protocol: bool,
}

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
    let ctx = Context {
        config,
        trade,
        store,
        request_id,
    };
    match exchange(config, outbound, &expect, limits).await {
        Err(failure) => ctx.failed(&failure).await,
        Ok(exchanged) => {
            let mut outcome = ctx.answer(&exchanged).await;
            outcome.add_warnings(exchanged.warnings());
            outcome
        }
    }
}

struct Context<'a> {
    config: &'a PublicReadConfig,
    trade: &'a Trade,
    store: &'a Store,
    request_id: u64,
}

impl Context<'_> {
    /// The index is spent: record it without an order. `Applied` or `Failed`.
    async fn spend_index(&self) -> Persistence {
        match self.store.advance(self.trade.index).await {
            Ok(()) => Persistence::Applied,
            Err(_) => Persistence::Failed,
        }
    }

    /// No reply was obtained. Nothing published is a plain error; anything
    /// else may have reached the node.
    async fn failed(&self, failure: &ExchangeFailure) -> NewOutcome {
        if failure.publication() == Publication::NotAttempted {
            return unsent(failure.code(), failure.warnings().clone());
        }
        let meta = ExecutionMeta::write(
            failure.publication(),
            failure.daemon_acceptance(),
            self.spend_index().await,
        );
        Envelope::uncertain(
            COMMAND,
            failure.code(),
            None,
            failure.warnings().clone(),
            meta,
        )
    }

    async fn answer(&self, exchanged: &Exchanged) -> NewOutcome {
        let kind = exchanged.reply().get_inner_message_kind();
        match (&kind.action, &kind.payload) {
            (Action::NewOrder, Some(Payload::Order(order))) if order.id.is_some() => {
                self.created(order).await
            }
            (Action::PayBondInvoice, _) => {
                let meta = ExecutionMeta::write(
                    Publication::Published,
                    DaemonAcceptance::Accepted,
                    self.spend_index().await,
                );
                Envelope::error(
                    Some(COMMAND),
                    ErrorCode::BondRequired,
                    Warnings::new(),
                    meta,
                )
            }
            (Action::CantDo, payload) => {
                let meta = ExecutionMeta::write(
                    Publication::Published,
                    DaemonAcceptance::Rejected,
                    Persistence::NotAttempted,
                );
                Envelope::refused(Some(COMMAND), &reason(payload), Warnings::new(), meta)
            }
            _ => self.malformed().await,
        }
    }

    /// The node answered but not with a usable order: it may hold one.
    async fn malformed(&self) -> NewOutcome {
        let meta = ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Unknown,
            self.spend_index().await,
        );
        Envelope::uncertain(
            COMMAND,
            ErrorCode::ReplyMalformed,
            None,
            Warnings::new(),
            meta,
        )
    }

    /// The node created `order`: keep it with the index in one transaction.
    async fn created(&self, order: &SmallOrder) -> NewOutcome {
        let Some((data, warnings)) = OrderNewData::from_created(
            &self.config.mostro_pubkey(),
            order,
            self.trade.index,
            &self.trade.keys.public_key(),
        ) else {
            return self.malformed().await;
        };
        match self
            .store
            .save_order(order, self.trade, self.request_id)
            .await
        {
            Ok(()) => Envelope::ok(
                COMMAND,
                data,
                warnings,
                ExecutionMeta::write(
                    Publication::Published,
                    DaemonAcceptance::Accepted,
                    Persistence::Applied,
                ),
            ),
            Err(_) => {
                // The transaction left nothing behind; at least spend the index.
                let _ = self.spend_index().await;
                Envelope::uncertain(
                    COMMAND,
                    ErrorCode::OrderNotSaved,
                    Some(data),
                    warnings,
                    ExecutionMeta::write(
                        Publication::Published,
                        DaemonAcceptance::Accepted,
                        Persistence::Failed,
                    ),
                )
            }
        }
    }
}

/// A failure before anything was published.
pub fn unsent(code: ErrorCode, warnings: Warnings) -> NewOutcome {
    Envelope::error(
        Some(COMMAND),
        code,
        warnings,
        ExecutionMeta::write(
            Publication::NotAttempted,
            DaemonAcceptance::Unknown,
            Persistence::NotAttempted,
        ),
    )
}

/// The node's refusal reason as its wire word, empty when it sent none.
fn reason(payload: &Option<Payload>) -> String {
    match payload {
        Some(Payload::CantDo(Some(reason))) => serde_json::to_value::<&CantDoReason>(reason)
            .ok()
            .and_then(|name| name.as_str().map(str::to_owned))
            .unwrap_or_default(),
        _ => String::new(),
    }
}
