//! The user's side in each stored order, for the `release` step of message
//! records (spec 021). Read from the local order rows before any relay is
//! contacted; public keys only.

use std::collections::HashMap;

use mostro_core::prelude::Kind;
use nostr_sdk::prelude::PublicKey;
use uuid::Uuid;

use crate::outcome::messages::TradeRole;
use crate::store::{Store, StoreError, StoredOrder};

/// The user's side in each stored order, with the trade key it was saved
/// with.
#[derive(Debug, Default)]
pub struct Roles(HashMap<Uuid, (PublicKey, TradeRole)>);

impl Roles {
    /// The rows with an id that parses, a trade key, a known `is_mine` and a
    /// kind that parses; the others give no side.
    pub fn from_rows(rows: &[StoredOrder]) -> Self {
        let side = |row: &StoredOrder| {
            let id = Uuid::parse_str(&row.id).ok()?;
            let kind = row.kind.parse::<Kind>().ok()?;
            let role = TradeRole::of_order(kind, row.is_mine?);
            Some((id, (row.trade_pubkey?, role)))
        };
        Self(rows.iter().filter_map(side).collect())
    }

    /// Every row of `store`.
    pub async fn load(store: &Store) -> Result<Self, StoreError> {
        store.order_rows().await.map(|rows| Self::from_rows(&rows))
    }

    /// The side in `order` (the order a message names), only when the
    /// message went to the key stored for it.
    pub fn of(&self, order: Option<Uuid>, key: &PublicKey) -> Option<TradeRole> {
        let (stored, role) = self.0.get(&order?)?;
        (stored == key).then_some(*role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::Keys;

    fn row(n: u128, kind: &str, key: Option<PublicKey>, is_mine: Option<bool>) -> StoredOrder {
        StoredOrder {
            id: Uuid::from_u128(n).to_string(),
            kind: kind.to_string(),
            status: "fiat-sent".to_string(),
            trade_pubkey: key,
            created_at: Some(1_792_108_800),
            is_mine,
        }
    }

    #[test]
    fn a_role_needs_the_kind_the_maker_flag_and_the_same_key() {
        let (ours, other) = (Keys::generate().public_key(), Keys::generate().public_key());
        let mut bad_id = row(9, "sell", Some(ours), Some(true));
        bad_id.id = "not-a-uuid".to_string();
        let roles = Roles::from_rows(&[
            row(1, "sell", Some(ours), Some(true)),
            row(2, "buy", Some(ours), Some(false)),
            row(3, "buy", Some(ours), Some(true)),
            row(4, "sell", Some(ours), None),
            row(5, "swap", Some(ours), Some(true)),
            row(6, "sell", None, Some(true)),
            bad_id,
        ]);
        let of = |n: u128, key: &PublicKey| roles.of(Some(Uuid::from_u128(n)), key);
        assert_eq!(of(1, &ours), Some(TradeRole::Seller));
        assert_eq!(of(2, &ours), Some(TradeRole::Seller));
        assert_eq!(of(3, &ours), Some(TradeRole::Buyer));
        assert_eq!(of(1, &other), None, "another key");
        assert_eq!(of(4, &ours), None, "is_mine not recorded");
        assert_eq!(of(5, &ours), None, "an unknown kind");
        assert_eq!(of(6, &ours), None, "no key");
        assert_eq!(of(7, &ours), None, "no row");
        assert_eq!(roles.of(None, &ours), None, "no order named");
        assert_eq!(roles.0.len(), 3, "a bad id gives no side");
    }
}
