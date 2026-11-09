//! The counterparty's trade key of a stored order, for the peer chat.
//!
//! The `counterparty_pubkey` column is filled by the legacy `getdm` and, as a
//! cache, by `commands::order::counterparty::find_counterparty` with a key
//! learned from the node's messages. Only that column of one row is ever
//! written here.

use nostr_sdk::prelude::PublicKey;
use uuid::Uuid;

use super::{Store, StoreError};

impl Store {
    /// The counterparty key stored with the order `id`, or `None` when there is
    /// no such order, the column is empty or what it holds is not a key.
    /// Writes nothing and creates no file beside the store.
    pub async fn counterparty(&self, id: Uuid) -> Result<Option<PublicKey>, StoreError> {
        let pool = self.open_read_only().await?;
        let stored: Result<Option<Option<String>>, _> =
            sqlx::query_scalar("SELECT counterparty_pubkey FROM orders WHERE id = ?1")
                .bind(id.to_string())
                .fetch_optional(&pool)
                .await;
        pool.close().await;
        let stored = stored.map_err(|_| StoreError::Unreadable)?;
        Ok(stored.flatten().and_then(|key| PublicKey::parse(&key).ok()))
    }

    /// Store `key`, as canonical hex, as the counterparty of the order `id`.
    /// Fails when there is no such order; touches no other column or row.
    pub async fn set_counterparty(&self, id: Uuid, key: &PublicKey) -> Result<(), StoreError> {
        let pool = self.open().await?;
        let done = sqlx::query("UPDATE orders SET counterparty_pubkey = ?1 WHERE id = ?2")
            .bind(key.to_hex())
            .bind(id.to_string())
            .execute(&pool)
            .await;
        pool.close().await;
        match done {
            Ok(done) if done.rows_affected() == 1 => Ok(()),
            _ => Err(StoreError::Unwritable),
        }
    }
}
