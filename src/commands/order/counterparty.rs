//! The counterparty of an order the user holds: the other side's trade key,
//! which the peer chat of the order is encrypted to.
//!
//! The order row's `counterparty_pubkey`, when it parses and is not our own
//! trade key, is used as it is, and no relay is contacted (step 1 of #245).
//! Otherwise all the node's mail to the order's trade key is read once, since
//! the epoch (the read asks each relay for at most its newest 50 messages),
//! and the newest message sent after the escrow lock names the counterparty
//! (`msg::counterparty`). A chat message never does. A key found that way is
//! saved to the row as a cache. A failed save changes nothing in the result
//! and is not reported, as on the legacy path: the next call looks the key up
//! again. No write is held across the read, and nothing is retried.
//!
//! A stored key is not checked against the mail. The legacy `getdm` stores the
//! key of the first node `Order` payload naming both trade keys and never
//! refreshes it. On Lightning that is always a post-lock message; a Cashu
//! node's `waiting-seller-to-pay` to the seller names both before the lock, so
//! there the row can name a taker who then left (mostro-cli cannot fund a
//! Cashu escrow). A trade whose key was never stored and whose lock-time mail
//! is gone (beyond those 50, or expired on relays that honor NIP-40 after the
//! node's `dm_days`, 30 by default) is `counterparty_unknown`; `getdmuser -p`
//! stays the fallback.

use nostr_sdk::prelude::{Keys, PublicKey, Timestamp};
use uuid::Uuid;

use crate::msg::counterparty::counterparty_in;
use crate::msg::inbox::read_inbox;
use crate::msg::{PublicReadConfig, ReadLimits};
use crate::outcome::{ErrorCode, Warnings};
use crate::store::{Store, StoreError};

/// The counterparty found, and the warnings of the read of the node's mail
/// (none when the row held the key).
#[derive(Clone, Debug)]
pub struct Counterparty {
    key: PublicKey,
    warnings: Warnings,
}

impl Counterparty {
    pub fn key(&self) -> PublicKey {
        self.key
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// Why the counterparty is not known: a store or read failure with its own
/// code, or `counterparty_unknown`; with the warnings of the read, if one ran.
#[derive(Clone, Debug)]
pub struct LookupFailure {
    code: ErrorCode,
    warnings: Warnings,
}

impl LookupFailure {
    fn new(code: ErrorCode, warnings: Warnings) -> Self {
        Self { code, warnings }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn warnings(&self) -> &Warnings {
        &self.warnings
    }
}

/// Find the counterparty of `order`, whose stored trade key is `trade`: the
/// row's, else the one the node's post-lock mail names (see the module
/// documentation).
///
/// Store failures are `identity_missing` (no store) or `store_unreadable`; a
/// failed read keeps its code (`relay_connection_failed`, `read_timeout`, ...);
/// no stored key and no post-lock message is `counterparty_unknown`.
pub async fn find_counterparty(
    order: Uuid,
    trade: &Keys,
    config: &PublicReadConfig,
    store: &Store,
    limits: &ReadLimits,
    now: Timestamp,
) -> Result<Counterparty, LookupFailure> {
    let ours = trade.public_key();
    let stored = store
        .counterparty(order)
        .await
        .map_err(|error| LookupFailure::new(store_code(error), Warnings::new()))?
        .filter(|key| *key != ours);
    if let Some(key) = stored {
        return Ok(Counterparty {
            key,
            warnings: Warnings::new(),
        });
    }
    let keys = [(None, trade.clone())];
    let inbox = read_inbox(config, &keys, Timestamp::from(0), now, limits)
        .await
        .map_err(|failure| LookupFailure::new(failure.code(), failure.warnings().clone()))?;
    let Some(key) = counterparty_in(&inbox.mail, order, &ours) else {
        return Err(LookupFailure::new(
            ErrorCode::CounterpartyUnknown,
            inbox.warnings,
        ));
    };
    // A cache only (see the module documentation): its result is ignored.
    let _ = store.set_counterparty(order, &key).await;
    Ok(Counterparty {
        key,
        warnings: inbox.warnings,
    })
}

fn store_code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable | StoreError::Unreadable => ErrorCode::StoreUnreadable,
    }
}
