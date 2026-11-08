//! `dispute.open`: open a dispute on an order the user holds. One `dispute`
//! request with no payload, signed by the order's stored key; the node answers
//! the initiator with `dispute-initiated-by-you` and the dispute id, and the
//! local status becomes `dispute`. The node answers a second dispute on the
//! same order with silence, which is `uncertain`, never retried.

use mostro_core::order::Status;
use mostro_core::prelude::Action;
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use super::held::{send_held, HeldRequest, Route};
use super::WriteOptions;
use crate::msg::{ExchangeLimits, Lane, PublicReadConfig};
use crate::outcome::dispute::{DisputeOpenData, COMMAND, REPLIES};
use crate::outcome::Envelope;
use crate::store::Store;

pub type DisputeOutcome = Envelope<DisputeOpenData, ()>;

/// Open a dispute on `order`.
pub async fn dispute_open(
    order: Uuid,
    config: &PublicReadConfig,
    identity: &Keys,
    store: &Store,
    options: WriteOptions,
    limits: &ExchangeLimits,
) -> DisputeOutcome {
    let request = HeldRequest {
        command: COMMAND,
        order,
        action: Action::Dispute,
        payload: None,
        replies: REPLIES.to_vec(),
        // The order is open, so the node knows its trade key.
        lane: Lane::Known,
    };
    let route = Route {
        config,
        identity,
        store,
        options,
        limits,
    };
    let node = config.mostro_pubkey();
    send_held(request, route, |action, payload, trade| {
        DisputeOpenData::from_reply(&node, order, (action, payload), trade)
            .map(|data| (data, Some(Status::Dispute)))
    })
    .await
}
