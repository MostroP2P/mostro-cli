//! `order.fiat-sent`, `order.release` and `order.cancel`: take one step on an
//! order the user already holds.
//!
//! One signed request goes out through `msg::exchange`, signed by the order's
//! stored trade key under the identity seal, with the order id, no payload and
//! no trade index: these commands never spend an index, so a failed run leaves
//! the database as it was. The node echoes the request id in every reply to the
//! sender, so the reply is selected by it; the order it names is checked
//! afterwards. After an accepted step the local status follows the node's; a
//! failed update is a warning, not a failure, because the node already acted.
//! A range order the user made, with room left, is continued: the maker's own
//! step (`release` of a sell order, `fiat-sent` of a buy order) carries a fresh
//! trade key, reserved right before publishing and spent once the request was
//! published and the node did not provably refuse it, as for `order new`. The
//! node creates the child order for that key; `order sync` gives it a local
//! row. The request is never retried. The command prints nothing and never
//! touches a client.

use mostro_core::prelude::{Action, Message, Payload};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use super::write::{reason, unsent};
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Expect, Outbound, PublicReadConfig};
use crate::outcome::step::{StepData, StepInput};
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, WarningCode,
    Warnings,
};
use crate::store::{Store, StoreError, Trade};

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
            next.keys.public_key().to_string(),
            u32::try_from(next.index).unwrap_or(u32::MAX),
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
            return unsent(command, failure.code(), failure.warnings().clone())
        }
        // Published, or possibly published: the node may have acted, so the key
        // it may hold is spent.
        Err(failure) => {
            let persistence = spend(store, next.as_ref()).await;
            return Envelope::uncertain(
                command,
                failure.code(),
                None,
                failure.warnings().clone(),
                ExecutionMeta::write(
                    failure.publication(),
                    failure.daemon_acceptance(),
                    persistence,
                ),
            );
        }
    };
    let reply = exchanged.reply().get_inner_message_kind();
    if reply.action == Action::CantDo {
        // A refusal took nothing: the index stays unspent.
        return refused(command, &reason(&reply.payload));
    }
    let reserved = next.as_ref().map(|next| next.index);
    let Some((data, status)) = StepData::from_reply(
        &config.mostro_pubkey(),
        input,
        (&reply.action, &reply.payload, reply.id),
        &trade.public_key(),
        reserved,
    ) else {
        let persistence = spend(store, next.as_ref()).await;
        return malformed(command, persistence);
    };
    let mut warnings = Warnings::new();
    let mut persistence = Persistence::NotAttempted;
    if let Some(status) = status {
        persistence = match store.set_status(input.order(), status).await {
            Ok(()) => Persistence::Applied,
            Err(_) => {
                warnings.add(WarningCode::LocalStatusStale);
                Persistence::Failed
            }
        };
    }
    if next.is_some() {
        match spend(store, next.as_ref()).await {
            Persistence::Failed => {
                warnings.add(WarningCode::TradeIndexNotRecorded);
                persistence = Persistence::Failed;
            }
            _ if persistence == Persistence::NotAttempted => persistence = Persistence::Applied,
            _ => {}
        }
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

/// The next trade the request carries, when it is the maker's step on a range
/// order of the user's with room left. Writes nothing: the index is spent
/// only once the request was published.
async fn next_trade(input: &StepInput, store: &Store) -> Result<Option<Trade>, ErrorCode> {
    let range = match store.maker_range(input.order()).await {
        Ok(range) => range,
        Err(_) => return Err(ErrorCode::StoreUnwritable),
    };
    let Some(range) = range else { return Ok(None) };
    if !input.kind().sends_key(range.sells) {
        return Ok(None);
    }
    store
        .next_trade()
        .await
        .map(Some)
        .map_err(|_| ErrorCode::StoreUnwritable)
}

/// Record that the reserved index is spent. `NotAttempted` when none was
/// reserved.
async fn spend(store: &Store, next: Option<&Trade>) -> Persistence {
    match next {
        None => Persistence::NotAttempted,
        Some(next) => match store.advance(next.index).await {
            Ok(()) => Persistence::Applied,
            Err(_) => Persistence::Failed,
        },
    }
}

/// The node refused: nothing changes locally.
fn refused(command: &'static str, reason: &str) -> StepOutcome {
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
fn malformed(command: &'static str, persistence: Persistence) -> StepOutcome {
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
