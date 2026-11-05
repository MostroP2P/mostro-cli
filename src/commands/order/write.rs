//! The outcomes every command that publishes one request and writes locally
//! shares: what to report, and what to record, for each way the exchange can
//! end. The index policy lives here: the trade index is reserved, and so
//! recorded as spent, right before publishing, so a key the node may have seen
//! is never reused, even when the run dies before the reply. A request that
//! never left and a refusal give it back, unless another command reserved one
//! meanwhile (the index is then skipped, which is harmless).

use mostro_core::order::SmallOrder;
use mostro_core::prelude::{CantDoReason, Payload};

use crate::msg::ExchangeFailure;
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, Warnings,
};
use crate::store::{Reservation, Role, Store, StoreError};

/// What a write command knows about the request it published.
pub(super) struct Sent<'a> {
    pub command: &'static str,
    /// The trade the request carries, already recorded as spent.
    pub reservation: &'a Reservation,
    pub store: &'a Store,
    pub request_id: u64,
    /// Whose order a saved row is.
    pub role: Role,
}

impl Sent<'_> {
    /// No reply was obtained. Nothing published is a plain error and gives the
    /// index back; anything else may have reached the node, which may hold the
    /// key, so the index stays spent.
    pub async fn failed<D>(&self, failure: &ExchangeFailure) -> Envelope<D, ()> {
        if failure.publication() == Publication::NotAttempted {
            self.store.release(self.reservation).await;
            return unsent(self.command, failure.code(), failure.warnings().clone());
        }
        let meta = ExecutionMeta::write(
            failure.publication(),
            failure.daemon_acceptance(),
            Persistence::Applied,
        );
        Envelope::uncertain(
            self.command,
            failure.code(),
            None,
            failure.warnings().clone(),
            meta,
        )
    }

    /// The node answered but not with a usable reply: it may hold the request,
    /// so the index stays spent.
    pub fn malformed<D>(&self) -> Envelope<D, ()> {
        let meta = ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Unknown,
            Persistence::Applied,
        );
        Envelope::uncertain(
            self.command,
            ErrorCode::ReplyMalformed,
            None,
            Warnings::new(),
            meta,
        )
    }

    /// The node refused before acting: the index is given back and nothing
    /// else is written.
    pub async fn refused<D>(&self, payload: &Option<Payload>) -> Envelope<D, ()> {
        self.store.release(self.reservation).await;
        let meta = ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Rejected,
            Persistence::NotAttempted,
        );
        Envelope::refused(Some(self.command), &reason(payload), Warnings::new(), meta)
    }

    /// The node accepted: keep `order` under the reserved trade. A failed save
    /// leaves the index spent; only the row is missing.
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
            .save_order_as(order, &self.reservation.trade, self.request_id, self.role)
            .await
        {
            Ok(()) => Envelope::ok(self.command, data, warnings, accepted(Persistence::Applied)),
            Err(_) => Envelope::uncertain(
                self.command,
                ErrorCode::OrderNotSaved,
                Some(data),
                warnings,
                accepted(Persistence::Failed),
            ),
        }
    }
}

/// The code of an unsent outcome when no trade could be reserved. The
/// reservation never reports `Unreadable`; it is `store_unwritable` here too,
/// a code both commands document.
pub(super) fn reserve_code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable | StoreError::Unreadable => ErrorCode::StoreUnwritable,
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
