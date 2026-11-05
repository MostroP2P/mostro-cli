//! The one local write of a request that creates an order: the order row and
//! the trade index.
//!
//! Unlike [`crate::identity`], this module opens `mcli.db` for writing, but it
//! never creates it, never changes permissions and never migrates. Every
//! operation opens the existing file, does one thing and closes it, so a failed
//! run leaves no connection or sidecar file behind. Errors carry no text from
//! the store. The trade key and index come from the same derivation as the rest
//! of the program ([`User::get_next_trade_keys`]).

use std::fmt;
use std::path::{Path, PathBuf};

use mostro_core::order::{SmallOrder, Status};
use nostr_sdk::prelude::Keys;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::User;
use crate::identity::{open_mode, resolve_dir, OpenMode};

/// `is_mine`, `min_amount`, `max_amount` and `fiat_amount` of a stored order.
type RangeRow = (Option<bool>, Option<i64>, Option<i64>, i64);

/// The rule of the legacy `fiatsent` and `release`: a next trade key is sent
/// when the order is flagged mine (any value), has a range, and the room left
/// in it, `max - fiat_amount`, still reaches the minimum.
pub fn range_has_room(
    is_mine: Option<bool>,
    min: Option<i64>,
    max: Option<i64>,
    fiat_amount: i64,
) -> bool {
    matches!((is_mine, min, max), (Some(_), Some(min), Some(max)) if max - fiat_amount >= min)
}

/// Why the store could not be read or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// No data directory, no `mcli.db`, or no user row in it.
    Missing,
    /// The store exists but cannot be opened, read or written.
    Unwritable,
    /// The store opened but what is in it cannot be read or used.
    Unreadable,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "no local store exists",
            Self::Unwritable => "the local store cannot be written",
            Self::Unreadable => "the local store cannot be read",
        })
    }
}

impl std::error::Error for StoreError {}

/// The key and index of the next order. The key is secret; `Debug` shows the
/// index only.
pub struct Trade {
    pub index: i64,
    pub keys: Keys,
}

impl fmt::Debug for Trade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Trade").field("index", &self.index).finish()
    }
}

/// The trade keys of the newest indexes, newest first. The keys are secret;
/// `Debug` shows the indexes only.
pub struct TradeKeys {
    pub keys: Vec<(i64, Keys)>,
    /// How many older indexes were not derived.
    pub not_read: usize,
}

impl fmt::Debug for TradeKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TradeKeys")
            .field(
                "indexes",
                &self.keys.iter().map(|(index, _)| index).collect::<Vec<_>>(),
            )
            .field("not_read", &self.not_read)
            .finish()
    }
}

/// The existing store in a data directory.
#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            path: dir.join("mcli.db"),
        }
    }

    /// The store in the configured data directory (`MCLI_DIR`, else `~/.mcli`).
    pub fn configured() -> Result<Self, StoreError> {
        let env = std::env::var("MCLI_DIR").ok();
        resolve_dir(env.as_deref(), dirs::home_dir())
            .map(|dir| Self::in_dir(&dir))
            .ok_or(StoreError::Missing)
    }

    async fn open(&self) -> Result<SqlitePool, StoreError> {
        match std::fs::metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(StoreError::Unwritable),
            Err(_) => return Err(StoreError::Missing),
        }
        let options = SqliteConnectOptions::new()
            .filename(&self.path)
            .create_if_missing(false);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|_| StoreError::Unwritable)
    }

    /// Open the store for reading without creating or deleting a file beside
    /// it, so a read works in a directory it may not write. A `-wal` file
    /// without its `-shm` file is unreadable here: unlike the mnemonic, the
    /// trade index changes, and the WAL may hold a newer one that only an
    /// open creating the `-shm` file could see.
    async fn open_read_only(&self) -> Result<SqlitePool, StoreError> {
        match std::fs::metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(StoreError::Unreadable),
            Err(_) => return Err(StoreError::Missing),
        }
        let immutable = match open_mode(&self.path) {
            Some(OpenMode::ReadOnly) => false,
            Some(OpenMode::Immutable) => true,
            Some(OpenMode::MainFileOnly) | None => return Err(StoreError::Unreadable),
        };
        let options = SqliteConnectOptions::new()
            .filename(&self.path)
            .read_only(true)
            .immutable(immutable)
            .create_if_missing(false);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|_| StoreError::Unreadable)
    }

    /// Fail now, before anything is published, if the store cannot be written:
    /// the file must accept a write (SQLite silently falls back to read-only on
    /// a file it may not write, so its own checks cannot say) and the write
    /// lock must be free. No byte changes.
    pub async fn ensure_writable(&self) -> Result<(), StoreError> {
        let pool = self.open().await?;
        if std::fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .is_err()
        {
            pool.close().await;
            return Err(StoreError::Unwritable);
        }
        let result = sqlx::query("BEGIN IMMEDIATE; ROLLBACK;")
            .execute(&pool)
            .await
            .map(|_| ())
            .map_err(|_| StoreError::Unwritable);
        pool.close().await;
        result
    }

    /// The trade key and index the next order would use: the recorded index plus
    /// one. Writes nothing.
    pub async fn next_trade(&self) -> Result<Trade, StoreError> {
        let pool = self.open().await?;
        let trade = User::get_next_trade_keys(&pool).await;
        pool.close().await;
        trade
            .map(|(keys, index)| Trade { index, keys })
            .map_err(|_| StoreError::Unwritable)
    }

    /// Record that `index` is spent without saving an order, so the next order
    /// uses a fresh key. Never moves the recorded index backwards.
    pub async fn advance(&self, index: i64) -> Result<(), StoreError> {
        let pool = self.open().await?;
        let result = advance_in(&pool, index).await;
        pool.close().await;
        result
    }

    /// The trade keys of the newest `newest` indexes up to the recorded last
    /// index, newest first, and how many older ones were left out. Derived from
    /// the identity's mnemonic, so a trade whose order was never saved has its
    /// key too. Writes nothing.
    pub async fn trade_keys(&self, newest: usize) -> Result<TradeKeys, StoreError> {
        let pool = self.open_read_only().await?;
        let derived = derive_trade_keys(&pool, newest).await;
        pool.close().await;
        derived
    }

    /// The trade key stored with the order `id`, or `None` when there is no
    /// such order, it has no key or the key is unusable. Writes nothing. The
    /// secret never leaves this module except as the key itself.
    pub async fn order_keys(&self, id: Uuid) -> Result<Option<Keys>, StoreError> {
        let pool = self.open().await?;
        let stored: Result<Option<Option<String>>, _> =
            sqlx::query_scalar("SELECT trade_keys FROM orders WHERE id = ?1")
                .bind(id.to_string())
                .fetch_optional(&pool)
                .await;
        pool.close().await;
        let stored = stored.map_err(|_| StoreError::Unwritable)?;
        Ok(stored
            .flatten()
            .and_then(|secret| Keys::parse(&secret).ok()))
    }

    /// Whether `fiat-sent` and `release` for the stored order `id` must carry a
    /// next trade key: the user made a range order with room left in it.
    /// `Ok(false)` when there is no such order. Writes nothing.
    pub async fn range_has_room(&self, id: Uuid) -> Result<bool, StoreError> {
        let pool = self.open().await?;
        let row: Result<Option<RangeRow>, _> = sqlx::query_as(
            "SELECT is_mine, min_amount, max_amount, fiat_amount FROM orders WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&pool)
        .await;
        pool.close().await;
        let row = row.map_err(|_| StoreError::Unwritable)?;
        Ok(row.is_some_and(|(is_mine, min, max, fiat)| range_has_room(is_mine, min, max, fiat)))
    }

    /// Set the status of the stored order `id`. Fails when there is no such
    /// order; touches nothing else, the trade index included.
    pub async fn set_status(&self, id: Uuid, status: Status) -> Result<(), StoreError> {
        let pool = self.open().await?;
        let done = sqlx::query("UPDATE orders SET status = ?1 WHERE id = ?2")
            .bind(status.to_string())
            .bind(id.to_string())
            .execute(&pool)
            .await;
        pool.close().await;
        match done {
            Ok(done) if done.rows_affected() == 1 => Ok(()),
            _ => Err(StoreError::Unwritable),
        }
    }

    /// Save the order the node created and record its index, in one transaction:
    /// both are written or neither is.
    pub async fn save_order(
        &self,
        order: &SmallOrder,
        trade: &Trade,
        request_id: u64,
    ) -> Result<(), StoreError> {
        let pool = self.open().await?;
        let result = save_in(&pool, order, trade, request_id).await;
        pool.close().await;
        result
    }
}

async fn derive_trade_keys(pool: &SqlitePool, newest: usize) -> Result<TradeKeys, StoreError> {
    let user = User::get(pool).await.map_err(|error| {
        if crate::db::is_row_not_found(&error) {
            StoreError::Missing
        } else {
            StoreError::Unreadable
        }
    })?;
    let last = user.last_trade_index.unwrap_or(0).max(0);
    let wanted = i64::try_from(newest).unwrap_or(i64::MAX);
    let first = (last - wanted + 1).max(1);
    let mut keys = Vec::new();
    for index in (first..=last).rev() {
        let key = User::get_trade_keys(pool, index)
            .await
            .map_err(|_| StoreError::Unreadable)?;
        keys.push((index, key));
    }
    let not_read = usize::try_from(first - 1).unwrap_or(0);
    Ok(TradeKeys { keys, not_read })
}

async fn advance_in(pool: &SqlitePool, index: i64) -> Result<(), StoreError> {
    let done = sqlx::query(
        "UPDATE users SET last_trade_index = ?1 \
         WHERE COALESCE(last_trade_index, 0) < ?1",
    )
    .bind(index)
    .execute(pool)
    .await
    .map_err(|_| StoreError::Unwritable)?;
    // Nothing changed is fine when the index was already recorded; a missing
    // user row is not.
    if done.rows_affected() == 0 {
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await
            .map_err(|_| StoreError::Unwritable)?;
        if rows == 0 {
            return Err(StoreError::Missing);
        }
    }
    Ok(())
}

async fn save_in(
    pool: &SqlitePool,
    order: &SmallOrder,
    trade: &Trade,
    request_id: u64,
) -> Result<(), StoreError> {
    let (Some(id), Some(kind), Some(status)) =
        (order.id, order.kind.as_ref(), order.status.as_ref())
    else {
        return Err(StoreError::Unwritable);
    };
    let unwritable = |_| StoreError::Unwritable;
    let mut transaction = pool.begin().await.map_err(unwritable)?;
    sqlx::query(
        "INSERT INTO orders (id, kind, status, amount, min_amount, max_amount, fiat_code, \
         fiat_amount, payment_method, premium, trade_keys, is_mine, request_id, created_at, \
         expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?13, ?14) \
         ON CONFLICT(id) DO UPDATE SET kind = ?2, status = ?3, amount = ?4, min_amount = ?5, \
         max_amount = ?6, fiat_code = ?7, fiat_amount = ?8, payment_method = ?9, premium = ?10, \
         trade_keys = ?11, is_mine = 1, request_id = ?12, expires_at = ?14",
    )
    .bind(id.to_string())
    .bind(kind.to_string())
    .bind(status.to_string())
    .bind(order.amount)
    .bind(order.min_amount)
    .bind(order.max_amount)
    .bind(&order.fiat_code)
    .bind(order.fiat_amount)
    .bind(&order.payment_method)
    .bind(order.premium)
    .bind(trade.keys.secret_key().to_secret_hex())
    .bind(request_id as i64)
    .bind(chrono::Utc::now().timestamp())
    .bind(order.expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(unwritable)?;
    sqlx::query("UPDATE users SET last_trade_index = ?1 WHERE COALESCE(last_trade_index, 0) < ?1")
        .bind(trade.index)
        .execute(&mut *transaction)
        .await
        .map_err(unwritable)?;
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *transaction)
        .await
        .map_err(unwritable)?;
    if users == 0 {
        // Dropping the transaction rolls the order row back.
        return Err(StoreError::Missing);
    }
    transaction.commit().await.map_err(unwritable)
}
