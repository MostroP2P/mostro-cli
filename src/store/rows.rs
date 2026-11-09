//! The order rows of the store, read-only, for `order mine`, `order watch` and
//! the user's side in message records. The trade key's secret never leaves
//! this module: a row carries the public key only.

use nostr_sdk::prelude::{Keys, PublicKey};

use super::{Store, StoreError};

/// `id`, `kind`, `status`, `trade_keys`, `created_at` and `is_mine` of an
/// order row.
type Row = (
    String,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<bool>,
);

/// One order row, with the public half of its trade key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredOrder {
    pub id: String,
    pub kind: String,
    pub status: String,
    /// `None` when the row has no key or the stored one cannot be read.
    pub trade_pubkey: Option<PublicKey>,
    pub created_at: Option<i64>,
    /// The user made the order (`false`: took it); `None` when the row does
    /// not say. Rows written by 0.16.3 and earlier say "made" for every order.
    pub is_mine: Option<bool>,
}

impl Store {
    /// Every order row, in no particular order. Writes nothing and creates no
    /// file beside the store.
    pub async fn order_rows(&self) -> Result<Vec<StoredOrder>, StoreError> {
        let pool = self.open_read_only().await?;
        let rows: Result<Vec<Row>, _> =
            sqlx::query_as("SELECT id, kind, status, trade_keys, created_at, is_mine FROM orders")
                .fetch_all(&pool)
                .await;
        pool.close().await;
        let rows = rows.map_err(|_| StoreError::Unreadable)?;
        Ok(rows
            .into_iter()
            .map(
                |(id, kind, status, keys, created_at, is_mine)| StoredOrder {
                    id,
                    kind,
                    status,
                    trade_pubkey: keys
                        .and_then(|secret| Keys::parse(&secret).ok())
                        .map(|keys| keys.public_key()),
                    created_at,
                    is_mine,
                },
            )
            .collect())
    }
}
