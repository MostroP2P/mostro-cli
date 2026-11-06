//! `order.mine` data: the orders of the local store, with the index and public
//! key of the trade key each one was made or taken with.
//!
//! Only public values are serialized: never a private key, the mnemonic or an
//! invoice. A row is accepted only when its id, side and status are ones the
//! program knows; anything else means the store is damaged.

use nostr_sdk::prelude::PublicKey;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

use super::order::{SIDES, STATUSES};
use super::{schema, ErrorCode, WarningCode};

/// Canonical name of `order mine`.
pub const COMMAND: &str = "order.mine";

/// Codes that fail `order.mine`.
pub const ERRORS: [ErrorCode; 4] = [
    ErrorCode::InvalidArguments,
    ErrorCode::InternalError,
    ErrorCode::IdentityMissing,
    ErrorCode::StoreUnreadable,
];

/// Warnings of `order.mine` beyond the pilot pair.
pub const WARNINGS: [WarningCode; 1] = [WarningCode::TradeKeysLimited];

/// One order row of the local store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MineOrder {
    #[schemars(schema_with = "schema::uuid")]
    id: String,
    #[schemars(schema_with = "schema::side")]
    side: String,
    /// The local status, as the store records it.
    #[schemars(schema_with = "schema::status")]
    status: String,
    /// The index of the order's trade key; null when it is not among the
    /// derived indexes or the row has no key.
    #[schemars(schema_with = "schema::nullable_decimal")]
    trade_index: Option<String>,
    /// The public key of the order's trade key; null when the row has none.
    #[schemars(schema_with = "schema::nullable_pubkey")]
    trade_pubkey: Option<String>,
}

impl MineOrder {
    /// The row as the store holds it; `None` when its id, side or status is
    /// not one the program knows.
    pub fn from_row(
        id: &str,
        side: &str,
        status: &str,
        trade: Option<(&PublicKey, Option<i64>)>,
    ) -> Option<Self> {
        let id = Uuid::parse_str(id).ok()?;
        if !SIDES.contains(&side) || !STATUSES.contains(&status) {
            return None;
        }
        Some(Self {
            id: id.to_string(),
            side: side.to_string(),
            status: status.to_string(),
            trade_index: trade.and_then(|(_, index)| index).map(|i| i.to_string()),
            trade_pubkey: trade.map(|(key, _)| key.to_hex()),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn side(&self) -> &str {
        &self.side
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn trade_index(&self) -> Option<&str> {
        self.trade_index.as_deref()
    }

    pub fn trade_pubkey(&self) -> Option<&str> {
        self.trade_pubkey.as_deref()
    }
}

/// The orders of the local store, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OrderMineData {
    orders: Vec<MineOrder>,
}

impl OrderMineData {
    pub fn new(orders: Vec<MineOrder>) -> Self {
        Self { orders }
    }

    pub fn orders(&self) -> &[MineOrder] {
        &self.orders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::Keys;
    use serde_json::json;

    #[test]
    fn a_row_keeps_its_public_values_and_unknown_values_are_refused() {
        let key = Keys::generate().public_key();
        let id = "00000000-0000-0000-0000-00000000000A";
        let row = MineOrder::from_row(id, "sell", "pending", Some((&key, Some(3)))).unwrap();
        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            json!({"id": id.to_lowercase(), "side": "sell", "status": "pending",
                   "trade_index": "3", "trade_pubkey": key.to_hex()})
        );
        let keyless = MineOrder::from_row(id, "buy", "dispute", None).unwrap();
        assert_eq!(keyless.trade_index(), None);
        assert_eq!(keyless.trade_pubkey(), None);
        for (id, side, status) in [
            ("not-a-uuid", "sell", "pending"),
            (id, "both", "pending"),
            (id, "sell", "no-such-status"),
            (id, "Sell", "pending"),
        ] {
            assert!(
                MineOrder::from_row(id, side, status, None).is_none(),
                "{side} {status}"
            );
        }
    }
}
