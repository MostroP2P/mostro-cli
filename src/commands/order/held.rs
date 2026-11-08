//! A request on an order the user holds that spends no trade index: one signed
//! request through `msg::exchange`, signed by the order's stored trade key under
//! the identity seal, with the order id. The node echoes the request id to the
//! sender, so the reply is selected by it; the order it names is checked
//! afterwards, for a refusal as for an acceptance. An accepted reply may set
//! the local status; a failed update is a warning, not a failure, because the
//! node already acted. Never retried. Used by `dispute open` and `order rate`.

use mostro_core::order::Status;
use mostro_core::prelude::{Action, Message, MessageKind, Payload};
use nostr_sdk::prelude::{Keys, PublicKey};
use uuid::Uuid;

use super::write::{reason, unsent};
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Expect, Lane, Outbound, PublicReadConfig};
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, WarningCode,
    Warnings,
};
use crate::store::{Store, StoreError};

/// What is sent and which replies, besides `cant-do`, may answer it.
pub struct HeldRequest {
    pub command: &'static str,
    pub order: Uuid,
    pub action: Action,
    pub payload: Option<Payload>,
    pub replies: Vec<Action>,
    /// The node's proof-of-work lane for the order's trade key.
    pub lane: Lane,
}

/// Where the request goes and with what.
pub struct Route<'a> {
    pub config: &'a PublicReadConfig,
    pub identity: &'a Keys,
    pub store: &'a Store,
    pub options: WriteOptions,
    pub limits: &'a ExchangeLimits,
}

/// Send `request` and map an accepted reply with `accept`: the data and the
/// local status to record, or `None` for a reply that cannot be used. `accept`
/// gets the reply's action and payload and the order's trade public key.
pub async fn send_held<D>(
    request: HeldRequest,
    route: Route<'_>,
    accept: impl Fn(&Action, &Option<Payload>, &PublicKey) -> Option<(D, Option<Status>)>,
) -> Envelope<D, ()> {
    let command = request.command;
    let trade = match route.store.order_keys(request.order).await {
        Ok(Some(keys)) => keys,
        Ok(None) | Err(StoreError::Missing) => {
            return unsent(command, ErrorCode::OrderUnknown, Warnings::new())
        }
        Err(StoreError::Unwritable | StoreError::Unreadable) => {
            return unsent(command, ErrorCode::StoreUnwritable, Warnings::new())
        }
    };
    let request_id = Uuid::new_v4().as_u128() as u64;
    let outbound = Outbound {
        identity: route.identity,
        signer: &trade,
        recipient: route.config.mostro_pubkey(),
        message: Message::new_order(
            Some(request.order),
            Some(request_id),
            None,
            request.action,
            request.payload,
        ),
        pow: route.options.pow,
        lane: request.lane,
        check_protocol: route.options.check_protocol,
    };
    let mut allowed_actions = request.replies;
    allowed_actions.push(Action::CantDo);
    let expect = Expect {
        allowed_actions,
        by_order: None,
    };
    let exchanged = match exchange(route.config, outbound, &expect, route.limits).await {
        Ok(exchanged) => exchanged,
        Err(failure) if failure.publication() == Publication::NotAttempted => {
            return unsent(command, failure.code(), failure.warnings().clone())
                .with_work(failure.work());
        }
        Err(failure) => {
            return Envelope::uncertain(
                command,
                failure.code(),
                None,
                failure.warnings().clone(),
                ExecutionMeta::write(
                    failure.publication(),
                    failure.daemon_acceptance(),
                    Persistence::NotAttempted,
                ),
            )
            .with_work(failure.work());
        }
    };
    let reply = exchanged.reply().get_inner_message_kind();
    // A refusal, like an acceptance, counts only when it names the order;
    // anything else is an unusable reply and the node may have acted.
    if reply.action == Action::CantDo {
        return refusal(command, request.order, reply)
            .unwrap_or_else(|| malformed(command, Persistence::NotAttempted));
    }
    let accepted = (reply.id == Some(request.order))
        .then(|| accept(&reply.action, &reply.payload, &trade.public_key()))
        .flatten();
    let Some((data, status)) = accepted else {
        return malformed(command, Persistence::NotAttempted);
    };
    let mut warnings = Warnings::new();
    let mut persistence = Persistence::NotAttempted;
    if let Some(status) = status {
        persistence = match route.store.set_status(request.order, status).await {
            Ok(()) => Persistence::Applied,
            Err(_) => {
                warnings.add(WarningCode::LocalStatusStale);
                Persistence::Failed
            }
        };
    }
    warnings.merge(exchanged.warnings());
    Envelope::ok(
        command,
        data,
        warnings,
        ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Accepted,
            persistence,
        ),
    )
}

/// A `cant-do` reply to a request on `order`: the node's refusal, with its
/// reason, only when it names the order. `None` for one that names another
/// order or none: that is an unusable reply, and the node may have acted.
pub fn refusal<D>(
    command: &'static str,
    order: Uuid,
    reply: &MessageKind,
) -> Option<Envelope<D, ()>> {
    (reply.id == Some(order)).then(|| refused(command, &reason(&reply.payload)))
}

/// The node refused: nothing changes locally.
fn refused<D>(command: &'static str, reason: &str) -> Envelope<D, ()> {
    Envelope::refused(
        Some(command),
        reason,
        Warnings::new(),
        ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Rejected,
            Persistence::NotAttempted,
        ),
    )
}

/// The node answered with something unusable: it may have acted.
pub fn malformed<D>(command: &'static str, persistence: Persistence) -> Envelope<D, ()> {
    Envelope::uncertain(
        command,
        ErrorCode::ReplyMalformed,
        None,
        Warnings::new(),
        ExecutionMeta::write(
            Publication::Published,
            DaemonAcceptance::Unknown,
            persistence,
        ),
    )
}
