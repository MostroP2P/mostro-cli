//! `message.list`: read the messages the node sent to the user's trade keys.
//!
//! A pure read: the keys come from the identity in the store (opened without
//! writing) and the user's side in each order from its rows, one bounded relay
//! read follows, and each message the node signed becomes a typed record.
//! Nothing is written, printed or retried, and a repeated call returns the
//! same messages.

use chrono::{DateTime, Utc};
use nostr_sdk::prelude::{Keys, Timestamp};
use uuid::Uuid;

use super::roles::Roles;
use crate::msg::inbox::{read_inbox, Mail};
use crate::msg::{PublicReadConfig, ReadLimits};
use crate::outcome::messages::{MessageListData, MessageListInput, MessageRecord, MAX_KEYS};
use crate::outcome::order::Snapshot;
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, WarningCode, Warnings};
use crate::store::{Store, StoreError};

pub const COMMAND: &str = "message.list";

pub type ListOutcome = Envelope<MessageListData, Snapshot>;

/// Read the node's messages for `input`.
pub async fn message_list(
    input: &MessageListInput,
    config: &PublicReadConfig,
    store: &Store,
    limits: &ReadLimits,
    now: DateTime<Utc>,
) -> ListOutcome {
    let mut warnings = Warnings::new();
    let (keys, keys_not_read) = match keys_for(input, store).await {
        Ok(found) => found,
        Err(code) => return failed(code, warnings, None),
    };
    if keys_not_read > 0 {
        warnings.add(WarningCode::TradeKeysLimited);
    }
    let roles = match Roles::load(store).await {
        Ok(roles) => roles,
        Err(error) => return failed(store_code(error), warnings, None),
    };
    let seconds = u64::try_from(now.timestamp()).unwrap_or(0);
    let since = Timestamp::from(seconds.saturating_sub(input.since_minutes() as u64 * 60));
    let inbox = match read_inbox(config, &keys, since, Timestamp::from(seconds), limits).await {
        Ok(inbox) => inbox,
        Err(failure) => {
            warnings.merge(failure.warnings());
            return failed(failure.code(), warnings, failure.snapshot().cloned());
        }
    };
    warnings.merge(&inbox.warnings);
    let node = config.mostro_pubkey();
    let messages: Vec<MessageRecord> = inbox
        .mail
        .iter()
        .map(|mail| record(mail, &roles, &mut warnings))
        .collect();
    let data = MessageListData::new(
        &node,
        input,
        messages,
        inbox.truncated,
        (keys.len(), keys_not_read),
    );
    Envelope::ok(
        COMMAND,
        data,
        warnings,
        ExecutionMeta::read(Some(inbox.snapshot)),
    )
}

/// The record of one message of the node, as `message list` returns it,
/// with the user's side in its order from `roles`.
pub(crate) fn record(mail: &Mail, roles: &Roles, warnings: &mut Warnings) -> MessageRecord {
    let order = mail.message.get_inner_message_kind().id;
    MessageRecord::from_message(
        &mail.event,
        mail.created_at,
        &mail.message,
        mail.index,
        &mail.key,
        roles.of(order, &mail.key),
        warnings,
    )
}

/// The keys to ask about and how many older ones are left out: the newest
/// [`MAX_KEYS`], or only the key stored for the order when one is given.
async fn keys_for(
    input: &MessageListInput,
    store: &Store,
) -> Result<(Vec<(Option<i64>, Keys)>, usize), ErrorCode> {
    let Some(order) = input.order() else {
        let derived = store.trade_keys(MAX_KEYS).await.map_err(store_code)?;
        let not_read = derived.not_read;
        let keys = derived
            .keys
            .into_iter()
            .map(|(index, key)| (Some(index), key))
            .collect();
        return Ok((keys, not_read));
    };
    let (index, stored) = order_key(store, order).await?;
    Ok((vec![(index, stored)], 0))
}

/// The trade key stored for `order`, with its index when it is among the
/// newest [`MAX_KEYS`] derived keys: `order_unknown` without a row.
pub(crate) async fn order_key(
    store: &Store,
    order: Uuid,
) -> Result<(Option<i64>, Keys), ErrorCode> {
    let derived = store.trade_keys(MAX_KEYS).await.map_err(store_code)?;
    let stored = match store.order_keys(order).await {
        Ok(Some(keys)) => keys,
        Ok(None) => return Err(ErrorCode::OrderUnknown),
        Err(error) => return Err(store_code(error)),
    };
    let index = derived
        .keys
        .iter()
        .find(|(_, key)| key.public_key() == stored.public_key())
        .map(|(index, _)| *index);
    Ok((index, stored))
}

pub(crate) fn store_code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable | StoreError::Unreadable => ErrorCode::StoreUnreadable,
    }
}

fn failed(code: ErrorCode, warnings: Warnings, snapshot: Option<Snapshot>) -> ListOutcome {
    Envelope::error(Some(COMMAND), code, warnings, ExecutionMeta::read(snapshot))
}
