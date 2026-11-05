//! `order.sync`: give the orders the node announced to the user's trade keys a
//! local row.
//!
//! A range order's child order, a message the user's commands never waited for,
//! and any order the store lost are announced by the node with a `new-order`
//! message addressed to the trade key that owns them. This reads those
//! messages the way `message list` does (one bounded read, the node as the
//! author, the derived trade keys as recipients) and saves a row for every order
//! the store does not know, with the key of its index. It publishes nothing and
//! never replaces a row: a second run saves nothing. A save that fails stops the
//! run, and the rows already saved stay.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use mostro_core::order::SmallOrder;
use mostro_core::prelude::{Action, Payload};
use nostr_sdk::prelude::{Keys, Timestamp};
use uuid::Uuid;

use crate::msg::inbox::{read_inbox, Mail};
use crate::msg::{PublicReadConfig, ReadLimits};
use crate::outcome::messages::MAX_KEYS;
use crate::outcome::order::Snapshot;
use crate::outcome::sync::{AdoptedOrder, SyncData, SyncInput};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, Persistence, WarningCode, Warnings};
use crate::store::{Store, StoreError, Trade};

pub const COMMAND: &str = "order.sync";

pub type SyncOutcome = Envelope<SyncData, Snapshot>;

/// An order due for a row, with the key it was announced to.
struct Due {
    id: Uuid,
    order: SmallOrder,
    index: i64,
    request_id: u64,
}

/// Save the orders the node announced that the store does not know.
pub async fn order_sync(
    input: &SyncInput,
    config: &PublicReadConfig,
    store: &Store,
    limits: &ReadLimits,
    now: DateTime<Utc>,
) -> SyncOutcome {
    let mut warnings = Warnings::new();
    let derived = match store.trade_keys(MAX_KEYS).await {
        Ok(derived) => derived,
        Err(error) => return failed(store_code(error), warnings, None),
    };
    let known = match store.known_orders().await {
        Ok(known) => known,
        Err(error) => return failed(store_code(error), warnings, None),
    };
    if derived.not_read > 0 {
        warnings.add(WarningCode::TradeKeysLimited);
    }
    let keys: Vec<(Option<i64>, Keys)> = derived
        .keys
        .iter()
        .map(|(index, key)| (Some(*index), key.clone()))
        .collect();
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
    let due = due_orders(&inbox.mail, &known);

    let mut adopted = Vec::new();
    for item in due {
        let Some((_, keys)) = derived.keys.iter().find(|(index, _)| *index == item.index) else {
            continue;
        };
        let trade = Trade {
            index: item.index,
            keys: keys.clone(),
        };
        match store
            .adopt_order(&item.order, &trade, item.request_id)
            .await
        {
            Ok(true) => adopted.push(AdoptedOrder::new(
                item.id,
                &item.order,
                (item.index, &trade.keys.public_key()),
                &mut warnings,
            )),
            // Another run saved it first: nothing to do.
            Ok(false) => {}
            Err(_) => {
                let data = data(
                    config,
                    input,
                    adopted,
                    &inbox.mail,
                    inbox.truncated,
                    &derived,
                );
                return Envelope::uncertain(
                    COMMAND,
                    ErrorCode::OrderNotSaved,
                    Some(data),
                    warnings,
                    ExecutionMeta::sync(Some(inbox.snapshot), Persistence::Failed),
                );
            }
        }
    }
    let persistence = if adopted.is_empty() {
        Persistence::NotAttempted
    } else {
        Persistence::Applied
    };
    let data = data(
        config,
        input,
        adopted,
        &inbox.mail,
        inbox.truncated,
        &derived,
    );
    Envelope::ok(
        COMMAND,
        data,
        warnings,
        ExecutionMeta::sync(Some(inbox.snapshot), persistence),
    )
}

fn data(
    config: &PublicReadConfig,
    input: &SyncInput,
    adopted: Vec<AdoptedOrder>,
    mail: &[Mail],
    truncated: bool,
    derived: &crate::store::TradeKeys,
) -> SyncData {
    SyncData::new(
        &config.mostro_pubkey(),
        input,
        adopted,
        (mail.len(), truncated),
        (derived.keys.len(), derived.not_read),
    )
}

/// The orders announced by a `new-order` message whose id the store lacks,
/// each once (the newest announcement), oldest first. A message that names no
/// order, another id than its payload, or no key index is not usable.
fn due_orders(mail: &[Mail], known: &std::collections::HashSet<Uuid>) -> Vec<Due> {
    let mut found: BTreeMap<Uuid, (u64, Due)> = BTreeMap::new();
    for item in mail {
        let kind = item.message.get_inner_message_kind();
        let (Action::NewOrder, Some(Payload::Order(order)), Some(id), Some(index)) =
            (&kind.action, &kind.payload, kind.id, item.index)
        else {
            continue;
        };
        if order.id != Some(id)
            || known.contains(&id)
            || order.kind.is_none()
            || order.status.is_none()
        {
            continue;
        }
        let due = Due {
            id,
            order: order.clone(),
            index,
            request_id: kind.request_id.unwrap_or(0),
        };
        found.insert(id, (item.created_at, due));
    }
    let mut due: Vec<(u64, Due)> = found.into_values().collect();
    due.sort_by_key(|(at, due)| (*at, due.id));
    due.into_iter().map(|(_, due)| due).collect()
}

fn store_code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable | StoreError::Unreadable => ErrorCode::StoreUnreadable,
    }
}

fn failed(code: ErrorCode, warnings: Warnings, snapshot: Option<Snapshot>) -> SyncOutcome {
    Envelope::error(
        Some(COMMAND),
        code,
        warnings,
        ExecutionMeta::sync(snapshot, Persistence::NotAttempted),
    )
}
