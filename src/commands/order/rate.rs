//! `order.rate`: rate the counterparty of a finished trade the user holds. One
//! `rate-user` request with the rating, signed by the order's stored key; the
//! node acknowledges it with `rate-received` and the same rating. Nothing is
//! written locally. The node answers a second rating with silence, which is
//! `uncertain`, never retried.

use mostro_core::prelude::Action;
use nostr_sdk::prelude::Keys;

use super::held::{send_held, HeldRequest, Route};
use super::WriteOptions;
use crate::msg::{ExchangeLimits, Lane, PublicReadConfig};
use crate::outcome::rate::{RateData, RateInput, COMMAND, REPLIES};
use crate::outcome::Envelope;
use crate::store::Store;

pub type RateOutcome = Envelope<RateData, ()>;

/// Send the rating of `input`.
pub async fn order_rate(
    input: &RateInput,
    config: &PublicReadConfig,
    identity: &Keys,
    store: &Store,
    options: WriteOptions,
    limits: &ExchangeLimits,
) -> RateOutcome {
    let request = HeldRequest {
        command: COMMAND,
        order: input.order(),
        action: Action::RateUser,
        payload: Some(input.payload()),
        replies: REPLIES.to_vec(),
        // A rated order is finished: the node no longer counts its trade key as known.
        lane: Lane::FirstContact,
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
        RateData::from_reply(&node, input, (action, payload), trade).map(|data| (data, None))
    })
    .await
}
