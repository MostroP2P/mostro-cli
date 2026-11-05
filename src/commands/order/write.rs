//! The outcomes every command that publishes one request and writes locally
//! shares: what to report, and what to record, for each way the exchange can
//! end. The index policy lives here: the trade index is spent whenever the
//! request was published and the node did not provably refuse it, so a key the
//! node may have seen is never reused; a refusal and a request that never left
//! spend nothing.

use mostro_core::order::SmallOrder;
use mostro_core::prelude::{CantDoReason, Payload};

use crate::msg::ExchangeFailure;
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, Warnings,
};
use crate::store::{Store, Trade};

/// What a write command knows about the request it published.
pub(super) struct Sent<'a> {
    pub command: &'static str,
    pub trade: &'a Trade,
    pub store: &'a Store,
    pub request_id: u64,
}

impl Sent<'_> {
    /// The index is spent: record it without an order. `Applied` or `Failed`.
    pub async fn spend_index(&self) -> Persistence {
        match self.store.advance(self.trade.index).await {
            Ok(()) => Persistence::Applied,
            Err(_) => Persistence::Failed,
        }
    }

    /// No reply was obtained. Nothing published is a plain error; anything
    /// else may have reached the node.
    pub async fn failed<D>(&self, failure: &ExchangeFailure) -> Envelope<D, ()> {
        if failure.publication() == Publication::NotAttempted {
            return unsent(self.command, failure.code(), failure.warnings().clone());
        }
        let meta = ExecutionMeta::write(
            failure.publication(),
            failure.daemon_acceptance(),
            self.spend_index().await,
        );
        Envelope::uncertain(
            self.command,
            failure.code(),
            None,
            failure.warnings().clone(),
            meta,
        )
    }

    /// The node answered but not with a usable reply: it may hold the request.
    pub async fn malformed<D>(&self) -> Envelope<D, ()> {
        let meta = ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Unknown,
            self.spend_index().await,
        );
        Envelope::uncertain(
            self.command,
            ErrorCode::ReplyMalformed,
            None,
            Warnings::new(),
            meta,
        )
    }

    /// The node refused: nothing is written.
    pub fn refused<D>(&self, payload: &Option<Payload>) -> Envelope<D, ()> {
        let meta = ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Rejected,
            Persistence::NotAttempted,
        );
        Envelope::refused(Some(self.command), &reason(payload), Warnings::new(), meta)
    }

    /// The node accepted: keep `order` with the index in one transaction.
    pub async fn saved<D>(
        &self,
        order: &SmallOrder,
        data: D,
        warnings: Warnings,
    ) -> Envelope<D, ()> {
        let accepted = |persistence| {
            ExecutionMeta::write(
                Publication::Published,
                DaemonAcceptance::Accepted,
                persistence,
            )
        };
        match self
            .store
            .save_order(order, self.trade, self.request_id)
            .await
        {
            Ok(()) => Envelope::ok(self.command, data, warnings, accepted(Persistence::Applied)),
            Err(_) => {
                // The transaction left nothing behind; at least spend the index.
                let _ = self.spend_index().await;
                Envelope::uncertain(
                    self.command,
                    ErrorCode::OrderNotSaved,
                    Some(data),
                    warnings,
                    accepted(Persistence::Failed),
                )
            }
        }
    }
}

/// A failure before anything was published.
pub fn unsent<D>(command: &'static str, code: ErrorCode, warnings: Warnings) -> Envelope<D, ()> {
    Envelope::error(
        Some(command),
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
pub(super) fn reason(payload: &Option<Payload>) -> String {
    match payload {
        Some(Payload::CantDo(Some(reason))) => serde_json::to_value::<&CantDoReason>(reason)
            .ok()
            .and_then(|name| name.as_str().map(str::to_owned))
            .unwrap_or_default(),
        _ => String::new(),
    }
}
