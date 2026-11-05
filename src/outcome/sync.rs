//! `order.sync` input and data: the orders the node announced to the user's
//! trade keys that the store did not know yet, and what was read to find them.
//!
//! A failed check names the option, never its value. Everything in the data
//! comes from messages the node signed and addressed to one of the user's own
//! keys; no key material is serialized.

use std::fmt;

use mostro_core::order::SmallOrder;
use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::info::OrderRecord;
use super::messages::MAX_SINCE_MINUTES;
use super::{schema, ErrorCode, WarningCode, Warnings};

/// The window read by default: as far back as `message list` can look, since
/// an adoption may be due long after the step that caused it.
pub const DEFAULT_SINCE_MINUTES: i64 = MAX_SINCE_MINUTES;

/// Codes that fail `order.sync` with a plain error.
pub const ERRORS: [ErrorCode; 12] = [
    ErrorCode::InvalidArguments,
    ErrorCode::ConfigurationMissing,
    ErrorCode::ConfigurationInvalid,
    ErrorCode::RelayConnectionFailed,
    ErrorCode::RelayReadFailed,
    ErrorCode::ReadTimeout,
    ErrorCode::ReadLimitExceeded,
    ErrorCode::InternalError,
    ErrorCode::IdentityMissing,
    ErrorCode::IdentityUnreadable,
    ErrorCode::StoreUnreadable,
    ErrorCode::StoreUnwritable,
];

/// Codes that leave it `uncertain`: some rows were saved and one was not.
pub const UNCERTAIN: [ErrorCode; 1] = [ErrorCode::OrderNotSaved];

/// Warnings of `order.sync` beyond the pilot pair.
pub const WARNINGS: [WarningCode; 2] = [
    WarningCode::TradeKeysLimited,
    WarningCode::InvalidOrderTimestamp,
];

/// The option that failed validation. Its value is never part of the error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSync;

impl fmt::Display for InvalidSync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid value for --since")
    }
}

impl std::error::Error for InvalidSync {}

/// A validated `order sync`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncInput {
    since_minutes: i64,
}

impl SyncInput {
    pub fn validate(since: Option<i64>) -> Result<Self, InvalidSync> {
        let since_minutes = since.unwrap_or(DEFAULT_SINCE_MINUTES);
        if !(1..=MAX_SINCE_MINUTES).contains(&since_minutes) {
            return Err(InvalidSync);
        }
        Ok(Self { since_minutes })
    }

    pub fn since_minutes(&self) -> i64 {
        self.since_minutes
    }
}

/// An order the store did not know, saved from the node's announcement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AdoptedOrder {
    order: OrderRecord,
    /// The index of the trade key the order was announced to.
    #[schemars(schema_with = "schema::decimal")]
    trade_index: String,
    #[schemars(schema_with = "schema::pubkey_hex")]
    trade_pubkey: String,
}

impl AdoptedOrder {
    /// Summarize `order`, announced to the key `trade_pubkey` of index
    /// `trade_index`. A time outside four-digit UTC years adds the aggregate
    /// `invalid_order_timestamp` warning.
    pub fn new(
        id: Uuid,
        order: &SmallOrder,
        (trade_index, trade_pubkey): (i64, &PublicKey),
        warnings: &mut Warnings,
    ) -> Self {
        Self {
            order: OrderRecord::from_order(id, order, warnings),
            trade_index: trade_index.to_string(),
            trade_pubkey: trade_pubkey.to_hex(),
        }
    }

    pub fn order(&self) -> &OrderRecord {
        &self.order
    }

    pub fn trade_index(&self) -> &str {
        &self.trade_index
    }

    pub fn trade_pubkey(&self) -> &str {
        &self.trade_pubkey
    }
}

/// What `order sync` found and saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SyncData {
    #[schemars(schema_with = "schema::pubkey_hex")]
    node_pubkey: String,
    #[schemars(schema_with = "schema::decimal")]
    since_minutes: String,
    /// The orders saved by this run, oldest first; empty when nothing was due.
    adopted: Vec<AdoptedOrder>,
    /// The messages of the node read in the window.
    #[schemars(schema_with = "schema::decimal")]
    messages_read: String,
    /// The messages that led to no row: other actions and known orders.
    #[schemars(schema_with = "schema::decimal")]
    messages_ignored: String,
    /// The result may not be everything: a relay reached its per-relay limit or
    /// more than the record cap matched.
    truncated: bool,
    #[schemars(schema_with = "schema::decimal")]
    keys_read: String,
    #[schemars(schema_with = "schema::decimal")]
    keys_not_read: String,
}

impl SyncData {
    pub fn new(
        node: &PublicKey,
        input: &SyncInput,
        adopted: Vec<AdoptedOrder>,
        (messages_read, truncated): (usize, bool),
        (keys_read, keys_not_read): (usize, usize),
    ) -> Self {
        Self {
            node_pubkey: node.to_hex(),
            since_minutes: input.since_minutes.to_string(),
            messages_ignored: messages_read.saturating_sub(adopted.len()).to_string(),
            adopted,
            messages_read: messages_read.to_string(),
            truncated,
            keys_read: keys_read.to_string(),
            keys_not_read: keys_not_read.to_string(),
        }
    }

    pub fn adopted(&self) -> &[AdoptedOrder] {
        &self.adopted
    }

    pub fn messages_read(&self) -> &str {
        &self.messages_read
    }

    pub fn messages_ignored(&self) -> &str {
        &self.messages_ignored
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn keys_read(&self) -> &str {
        &self.keys_read
    }

    pub fn keys_not_read(&self) -> &str {
        &self.keys_not_read
    }

    pub fn since_minutes(&self) -> &str {
        &self.since_minutes
    }

    pub fn node_pubkey(&self) -> &str {
        &self.node_pubkey
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_defaults_to_the_longest_and_is_bounded() {
        assert_eq!(
            SyncInput::validate(None).unwrap().since_minutes(),
            MAX_SINCE_MINUTES
        );
        for good in [1, 60, MAX_SINCE_MINUTES] {
            assert!(SyncInput::validate(Some(good)).is_ok(), "{good}");
        }
        for bad in [0, -1, MAX_SINCE_MINUTES + 1, i64::MIN, i64::MAX] {
            let error = SyncInput::validate(Some(bad)).unwrap_err();
            assert_eq!(error.to_string(), "invalid value for --since", "{bad}");
        }
    }

    #[test]
    fn the_ignored_count_is_what_was_read_and_not_adopted() {
        let node = nostr_sdk::prelude::Keys::generate().public_key();
        let input = SyncInput::validate(None).unwrap();
        let data = SyncData::new(&node, &input, Vec::new(), (5, false), (3, 0));
        assert_eq!((data.messages_read(), data.messages_ignored()), ("5", "5"));
        assert_eq!((data.keys_read(), data.keys_not_read()), ("3", "0"));
        assert!(data.adopted().is_empty() && !data.truncated());
    }
}
