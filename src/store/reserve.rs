//! The trade index as a durable reservation. Reserving the next trade records
//! its index in one short write transaction that commits before anything is
//! published: the reservation is the spend. A run that dies after publishing
//! therefore never lets a later run reuse the key the node may hold, `order
//! sync` derives that key, and two runs at once take turns on SQLite's write
//! lock instead of sharing an index. A request that never left, or that the
//! node refused for any reason but a stale index, gives its index back with a
//! compare-and-swap, which succeeds only while no later reservation exists;
//! otherwise the index is skipped, a harmless gap (the node only needs an
//! index above its last one).

use std::fmt;

use sqlx::SqlitePool;

use super::{StoreError, Trade};
use crate::identity::{trade_keys_from_mnemonic, LAST_TRADE_INDEX};

/// The recorded index, raised to `?1` unless it is already there or beyond.
pub(super) const RAISE: &str =
    "UPDATE users SET last_trade_index = ?1 WHERE COALESCE(last_trade_index, 0) < ?1";

/// A trade whose index is already recorded as spent. The key is secret;
/// `Debug` shows the index only.
pub struct Reservation {
    pub trade: Trade,
    /// The recorded value before the reservation, restored by a give-back.
    previous: Option<i64>,
}

impl fmt::Debug for Reservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reservation")
            .field("index", &self.trade.index)
            .finish()
    }
}

/// Record the next index (the recorded one plus one, never below 1) in one
/// `BEGIN IMMEDIATE` transaction, then derive its key from the mnemonic read in
/// it. `IMMEDIATE` takes the write lock under the busy handler, where a
/// deferred transaction would fail at once on the upgrade from its read lock.
/// Nothing is returned unless the commit succeeded, so an index that was not
/// recorded is never published. The key is derived after the commit, so the
/// lock is held only for the read and the write: deriving inside it made 32
/// concurrent runs overrun the busy timeout in the test profile. A derivation
/// that fails (the mnemonic was already used for the identity) gives the
/// index back.
pub(super) async fn reserve_in(pool: &SqlitePool) -> Result<Reservation, StoreError> {
    let unwritable = |_| StoreError::Unwritable;
    let mut transaction = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(unwritable)?;
    let row: Option<(Option<i64>, Option<String>)> =
        sqlx::query_as("SELECT last_trade_index, mnemonic FROM users LIMIT 1")
            .fetch_optional(&mut *transaction)
            .await
            .map_err(unwritable)?;
    // Dropping the transaction on any early return rolls it back.
    let Some((previous, mnemonic)) = row else {
        return Err(StoreError::Missing);
    };
    let mnemonic = mnemonic.ok_or(StoreError::Unwritable)?;
    let index = previous.unwrap_or(0).max(0) + 1;
    if index > LAST_TRADE_INDEX {
        // No key exists past the last address of the derivation path.
        return Err(StoreError::Unwritable);
    }
    let raised = sqlx::query(RAISE)
        .bind(index)
        .execute(&mut *transaction)
        .await
        .map_err(unwritable)?;
    if raised.rows_affected() == 0 {
        return Err(StoreError::Unwritable);
    }
    transaction.commit().await.map_err(unwritable)?;
    match trade_keys_from_mnemonic(&mnemonic, index) {
        Ok(keys) => Ok(Reservation {
            trade: Trade { index, keys },
            previous,
        }),
        Err(_) => {
            give_back(pool, index, previous).await;
            Err(StoreError::Unwritable)
        }
    }
}

/// Give `reservation` back: restore the previous value, `NULL` included, only
/// while the recorded index is still the reserved one. `true` when it was given
/// back; otherwise, or on any error, the index stays spent.
pub(super) async fn release_in(pool: &SqlitePool, reservation: &Reservation) -> bool {
    give_back(pool, reservation.trade.index, reservation.previous).await
}

/// Put `previous` back while `index` is still the recorded one.
async fn give_back(pool: &SqlitePool, index: i64, previous: Option<i64>) -> bool {
    sqlx::query("UPDATE users SET last_trade_index = ?1 WHERE last_trade_index = ?2")
        .bind(previous)
        .bind(index)
        .execute(pool)
        .await
        .is_ok_and(|done| done.rows_affected() > 0)
}

/// Record `index` as spent without moving the recorded index backwards, and
/// return the index recorded afterwards. For the legacy writers, which learn an
/// index after the fact (a saved order, the node's `last-trade-index` reply).
/// A missing user row is `Missing`.
pub(crate) async fn raise_index_in(pool: &SqlitePool, index: i64) -> Result<i64, StoreError> {
    let unwritable = |_| StoreError::Unwritable;
    sqlx::query(RAISE)
        .bind(index)
        .execute(pool)
        .await
        .map_err(unwritable)?;
    let recorded: Option<Option<i64>> =
        sqlx::query_scalar("SELECT last_trade_index FROM users LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(unwritable)?;
    recorded
        .map(|last| last.unwrap_or(0))
        .ok_or(StoreError::Missing)
}
