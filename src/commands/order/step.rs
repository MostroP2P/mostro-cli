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
//! A range order of the user's with room left is not continued (no next trade
//! key is sent; spec 008): an accepted `fiat-sent` or `release` says so in a
//! warning. The request is never retried. The command prints nothing and never
//! touches a client.

use mostro_core::prelude::{Action, Message};
use nostr_sdk::prelude::Keys;
use uuid::Uuid;

use super::write::{reason, unsent};
use super::WriteOptions;
use crate::msg::{exchange, ExchangeLimits, Expect, Outbound, PublicReadConfig};
use crate::outcome::step::{StepData, StepInput, StepResult};
use crate::outcome::{
    DaemonAcceptance, Envelope, ErrorCode, ExecutionMeta, Persistence, Publication, WarningCode,
    Warnings,
};
use crate::store::{Store, StoreError};

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
            None,
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
        // Published, or possibly published: the node may have acted.
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
        }
    };
    let reply = exchanged.reply().get_inner_message_kind();
    if reply.action == Action::CantDo {
        return refused(command, &reason(&reply.payload));
    }
    let Some((data, status)) = StepData::from_reply(
        &config.mostro_pubkey(),
        input,
        (&reply.action, &reply.payload, reply.id),
        &trade.public_key(),
        None,
    ) else {
        return malformed(command);
    };
    let mut warnings = Warnings::new();
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
    let settled = matches!(data.result(), StepResult::FiatSent | StepResult::Released);
    if settled && store.range_has_room(input.order()).await == Ok(true) {
        warnings.add(WarningCode::RangeNotContinued);
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
fn malformed(command: &'static str) -> StepOutcome {
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
