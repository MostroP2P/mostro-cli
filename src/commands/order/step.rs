//! `order.fiat-sent`, `order.release` and `order.cancel`: take one step on an
//! order the user already holds.
//!
//! One signed request goes out through `msg::exchange`, signed by the order's
//! stored trade key under the identity seal, with the order id, no payload and
//! no trade index: apart from the continuation of a range order (below), these
//! commands spend no index, so a failed run leaves the database as it was. The
//! node echoes the request id in every reply to the sender, so the reply is
//! selected by it; the order it names is checked afterwards. After an accepted
//! step the local status follows the node's; a failed update is a warning, not
//! a failure, because the node already acted.
//! A range order the user made, with room left, is continued: the maker's own
//! step (`release` of a sell order, `fiat-sent` of a buy order) carries a fresh
//! trade key whose index is reserved, and so recorded as spent, right before
//! publishing, as for `order new`; a request that never left and a refusal
//! give it back. The node never records that index itself, so recording it
//! first is what keeps a later order from reusing the child's key, even when
//! this run dies before the reply. The node creates the child order for that
//! key; `order sync` gives it a local row. The request is never retried. The
//! command prints nothing and never touches a client.

use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use super::held::{malformed, refusal};
use super::write::unsent;
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Expect, Lane, Outbound, PublicReadConfig};
use crate::outcome::step::{StepData, StepInput};
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, WarningCode,
    Warnings,
};
use crate::store::{Reservation, Store, StoreError};

pub type StepOutcome = Envelope<StepData, ()>;

/// Take the step of `input` on its order.
pub async fn order_step(
    input: &StepInput,
    config: &PublicReadConfig,
    identity: &Keys,
    store: &Store,
    options: WriteOptions,
    limits: &ExchangeLimits,
) -> StepOutcome {
    let kind = input.kind();
    let command = kind.command();
    let trade = match store.order_keys(input.order()).await {
        Ok(Some(keys)) => keys,
        Ok(None) | Err(StoreError::Missing) => {
            return unsent(command, ErrorCode::OrderUnknown, Warnings::new())
        }
        Err(StoreError::Unwritable | StoreError::Unreadable) => {
            return unsent(command, ErrorCode::StoreUnwritable, Warnings::new())
        }
    };
    let next = match next_trade(input, store).await {
        Ok(next) => next,
        Err(code) => return unsent(command, code, Warnings::new()),
    };
    let request_id = Uuid::new_v4().as_u128() as u64;
    let payload = next.as_ref().map(|next| {
        Payload::NextTrade(
            next.trade.keys.public_key().to_string(),
            u32::try_from(next.trade.index).unwrap_or(u32::MAX),
        )
    });
    let outbound = Outbound {
        identity,
        signer: &trade,
        recipient: config.mostro_pubkey(),
        message: Message::new_order(
            Some(input.order()),
            Some(request_id),
            None,
            kind.action(),
            payload,
        ),
        pow: options.pow,
        lane: Lane::Known,
        check_protocol: options.check_protocol,
    };
    let mut allowed_actions = kind.replies();
    allowed_actions.push(Action::CantDo);
    let expect = Expect {
        allowed_actions,
        by_order: None,
    };
    let exchanged = match exchange(config, outbound, &expect, limits).await {
        Ok(exchanged) => exchanged,
        Err(failure) if failure.publication() == Publication::NotAttempted => {
            give_back(store, next.as_ref()).await;
            return unsent(command, failure.code(), failure.warnings().clone())
                .with_work(failure.work());
        }
        // Published, or possibly published: the node may have acted, so the key
        // it may hold stays spent.
        Err(failure) => {
            return Envelope::uncertain(
                command,
                failure.code(),
                None,
                failure.warnings().clone(),
                ExecutionMeta::write(
                    failure.publication(),
                    failure.daemon_acceptance(),
                    spent(next.as_ref()),
                ),
            )
            .with_work(failure.work());
        }
    };
    let reply = exchanged.reply().get_inner_message_kind();
    if reply.action == Action::CantDo {
        // Only a refusal that names the order is one; anything else may hide
        // an action, so a key that was sent stays spent.
        let Some(refused) = refusal(command, input.order(), reply) else {
            return malformed(command, spent(next.as_ref()));
        };
        // The node refuses before it acts (before it keeps a key or makes a
        // child), so the index is given back.
        give_back(store, next.as_ref()).await;
        return refused;
    }
    let reserved = next.as_ref().map(|next| next.trade.index);
    let Some((data, status)) = StepData::from_reply(
        &config.mostro_pubkey(),
        input,
        (&reply.action, &reply.payload, reply.id),
        &trade.public_key(),
        reserved,
    ) else {
        return malformed(command, spent(next.as_ref()));
    };
    let mut warnings = Warnings::new();
    let mut persistence = spent(next.as_ref());
    if let Some(status) = status {
        persistence = match store.set_status(input.order(), status).await {
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

/// The next trade the request carries, reserved (and so recorded as spent)
/// right before publishing, when it is the maker's step on a range order of the
/// user's with room left. Any other step reserves nothing and writes nothing.
async fn next_trade(input: &StepInput, store: &Store) -> Result<Option<Reservation>, ErrorCode> {
    let range = match store.maker_range(input.order()).await {
        Ok(range) => range,
        Err(_) => return Err(ErrorCode::StoreUnwritable),
    };
    let Some(range) = range else { return Ok(None) };
    if !input.kind().sends_key(range.sells) {
        return Ok(None);
    }
    store
        .reserve_trade()
        .await
        .map(Some)
        .map_err(|_| ErrorCode::StoreUnwritable)
}

/// What the request wrote locally before it was published: the reserved index,
/// if any.
fn spent(next: Option<&Reservation>) -> Persistence {
    match next {
        Some(_) => Persistence::Applied,
        None => Persistence::NotAttempted,
    }
}

/// Give the reserved index back, if any, after the request provably had no
/// effect.
async fn give_back(store: &Store, next: Option<&Reservation>) {
    if let Some(next) = next {
        store.release(next).await;
    }
}
