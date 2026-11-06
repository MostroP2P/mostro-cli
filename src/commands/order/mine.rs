//! `order.mine`: the orders of the local store, read-only, with no node, relay
//! or configuration. The store keeps each order's trade key but not its index;
//! the index is found by deriving the keys of the newest recorded indexes, as
//! `message list` does, and matching them. The command never prints or exits.

use std::collections::HashMap;

use crate::outcome::mine::{MineOrder, OrderMineData, COMMAND};
use crate::outcome::{Envelope, ErrorCode, ExecutionMeta, WarningCode, Warnings};
use crate::store::{Store, StoreError, StoredOrder, TradeKeys};

pub type MineOutcome = Envelope<OrderMineData, ()>;

/// How many of the newest trade keys are derived to find the rows' indexes.
pub const TRADE_KEY_LIMIT: usize = 200;

/// What `order mine` reads: the store, or in tests a double that lets another
/// command write between the two reads.
trait OrderSource {
    async fn order_rows(&self) -> Result<Vec<StoredOrder>, StoreError>;
    async fn trade_keys(&self, newest: usize) -> Result<TradeKeys, StoreError>;
}

impl OrderSource for Store {
    async fn order_rows(&self) -> Result<Vec<StoredOrder>, StoreError> {
        Store::order_rows(self).await
    }

    async fn trade_keys(&self, newest: usize) -> Result<TradeKeys, StoreError> {
        Store::trade_keys(self, newest).await
    }
}

/// The orders of `store`, newest first.
pub async fn order_mine(store: Option<&Store>) -> MineOutcome {
    match store {
        Some(store) => list(store).await,
        None => failed(ErrorCode::IdentityMissing),
    }
}

async fn list(store: &impl OrderSource) -> MineOutcome {
    // Rows first: an index is reserved before its row is saved and only grows,
    // so the keys read afterwards cover every row read here, even when another
    // command saves an order between the two reads.
    let rows = match store.order_rows().await {
        Ok(rows) => rows,
        Err(error) => return failed(code(error)),
    };
    let keys = match store.trade_keys(TRADE_KEY_LIMIT).await {
        Ok(keys) => keys,
        Err(error) => return failed(code(error)),
    };
    let indexes: HashMap<_, _> = keys
        .keys
        .iter()
        .map(|(index, keys)| (keys.public_key(), *index))
        .collect();
    let mut found: Vec<(Option<i64>, StoredOrder)> = rows
        .into_iter()
        .map(|row| {
            let index = row.trade_pubkey.and_then(|key| indexes.get(&key).copied());
            (index, row)
        })
        .collect();
    // Newest first: the highest index, then the rows whose index is unknown by
    // creation time, newest first, then by id.
    found.sort_by(|(a, x), (b, y)| {
        b.cmp(a)
            .then(y.created_at.cmp(&x.created_at))
            .then(x.id.cmp(&y.id))
    });
    let mut warnings = Warnings::new();
    let mut orders = Vec::with_capacity(found.len());
    for (index, row) in &found {
        if index.is_none() && row.trade_pubkey.is_some() && keys.not_read > 0 {
            warnings.add(WarningCode::TradeKeysLimited);
        }
        let trade = row.trade_pubkey.as_ref().map(|key| (key, *index));
        match MineOrder::from_row(&row.id, &row.kind, &row.status, trade) {
            Some(order) => orders.push(order),
            None => return failed(ErrorCode::StoreUnreadable),
        }
    }
    Envelope::ok(
        COMMAND,
        OrderMineData::new(orders),
        warnings,
        ExecutionMeta::read(None),
    )
}

fn code(error: StoreError) -> ErrorCode {
    match error {
        StoreError::Missing => ErrorCode::IdentityMissing,
        StoreError::Unwritable | StoreError::Unreadable => ErrorCode::StoreUnreadable,
    }
}

fn failed(code: ErrorCode) -> MineOutcome {
    Envelope::error(
        Some(COMMAND),
        code,
        Warnings::new(),
        ExecutionMeta::read(None),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::{Keys, PublicKey};
    use std::sync::Mutex;
    use uuid::Uuid;

    /// A store that another `order new` writes to right after each read: the
    /// next index is reserved, then the order is saved with its key, as
    /// `Store::reserve_trade` and `Sent::saved` do.
    /// What the store holds: the recorded trade keys and the order rows.
    #[derive(Default)]
    struct Written {
        keys: Vec<(i64, Keys)>,
        rows: Vec<StoredOrder>,
    }

    struct Racing {
        state: Mutex<Written>,
    }

    impl Racing {
        fn new() -> Self {
            let racing = Self {
                state: Mutex::new(Written::default()),
            };
            racing.another_order();
            racing
        }

        fn another_order(&self) {
            let mut state = self.state.lock().unwrap();
            let index = state.keys.len() as i64 + 1;
            let keys = Keys::generate();
            state.keys.push((index, keys.clone()));
            state.rows.push(StoredOrder {
                id: Uuid::from_u128(index as u128).to_string(),
                kind: "sell".to_string(),
                status: "pending".to_string(),
                trade_pubkey: Some(keys.public_key()),
                created_at: Some(index),
            });
        }
    }

    impl OrderSource for Racing {
        async fn order_rows(&self) -> Result<Vec<StoredOrder>, StoreError> {
            let rows = self.state.lock().unwrap().rows.clone();
            self.another_order();
            Ok(rows)
        }

        async fn trade_keys(&self, _: usize) -> Result<TradeKeys, StoreError> {
            let mut keys = self.state.lock().unwrap().keys.clone();
            keys.reverse();
            self.another_order();
            Ok(TradeKeys { keys, not_read: 0 })
        }
    }

    /// A store that returns fixed rows and keys.
    struct Fixed {
        keys: Vec<(i64, Keys)>,
        rows: Vec<StoredOrder>,
    }

    impl OrderSource for Fixed {
        async fn order_rows(&self) -> Result<Vec<StoredOrder>, StoreError> {
            Ok(self.rows.clone())
        }

        async fn trade_keys(&self, _: usize) -> Result<TradeKeys, StoreError> {
            Ok(TradeKeys {
                keys: self.keys.clone(),
                not_read: 0,
            })
        }
    }

    /// FR-002: a known index first, then the rows whose index is unknown,
    /// newest `created_at` first, then by id.
    #[tokio::test]
    async fn unknown_indexes_follow_the_known_newest_first_then_by_id() {
        let known = Keys::generate();
        let row = |n: u128, key: Option<PublicKey>, created: i64| StoredOrder {
            id: Uuid::from_u128(n).to_string(),
            kind: "sell".to_string(),
            status: "pending".to_string(),
            trade_pubkey: key,
            created_at: Some(created),
        };
        let source = Fixed {
            keys: vec![(1, known.clone())],
            rows: vec![
                row(1, None, 10),
                row(3, None, 20),
                row(2, Some(Keys::generate().public_key()), 20),
                row(4, Some(known.public_key()), 5),
            ],
        };

        let outcome = list(&source).await;

        let ids: Vec<_> = outcome
            .data()
            .expect("a listing")
            .orders()
            .iter()
            .map(|order| order.id().to_string())
            .collect();
        assert_eq!(ids, [4, 2, 3, 1].map(|n| Uuid::from_u128(n).to_string()));
    }

    /// The index only grows and is reserved before the row is saved, so the
    /// keys read after the rows always cover them: no listed row may lack its
    /// index because an order landed between the two reads.
    #[tokio::test]
    async fn an_order_saved_between_the_reads_never_lacks_its_index() {
        let outcome = list(&Racing::new()).await;
        let data = outcome.data().expect("a listing");
        assert!(!data.orders().is_empty());
        for order in data.orders() {
            assert!(order.trade_index().is_some(), "{} has no index", order.id());
        }
        assert!(outcome.warnings().is_empty());
    }
}
