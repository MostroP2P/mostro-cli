//! The local writes of the requests that create or continue an order: the
//! trade index, reserved before publishing (`store/reserve.rs`), and the order row.
//!
//! Unlike [`crate::identity`], this module opens `mcli.db` for writing, but it
//! never creates it, never changes permissions and never migrates, except
//! [`create_store`], which makes a new one as the first run does (the data
//! directory owner-only, an existing one tightened to 0700 on Unix) and never
//! touches an existing store. Every
//! operation opens the existing file, does one thing and closes it, so a failed
//! run leaves no connection or sidecar file behind. Errors carry no text from
//! the store. The trade key and index come from the same derivation as the rest
//! of the program ([`crate::identity::trade_keys_from_mnemonic`]).

use std::fmt;
use std::path::{Path, PathBuf};

use mostro_core::order::{SmallOrder, Status};
use nostr_sdk::prelude::Keys;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::User;
use crate::identity::{open_mode, resolve_dir, OpenMode};
pub(crate) use create::{create_private_db_file, initialize};
pub use create::{create_store, CreateError};
pub(crate) use reserve::raise_index_in;
pub use reserve::Reservation;
use reserve::{release_in, reserve_in};
use save::{save_in, Existing};

mod create;
mod reserve;
mod save;

/// `is_mine`, `min_amount`, `max_amount`, `fiat_amount` and `kind` of a stored order.
type RangeRow = (Option<bool>, Option<i64>, Option<i64>, i64, Option<String>);

/// Whether the user's range order has room for a child order: the order is the
/// user's own (a taker continues nothing), has a range, and the room left in
/// it, `max - fiat_amount`, still reaches the minimum.
pub fn range_has_room(
    is_mine: Option<bool>,
    min: Option<i64>,
    max: Option<i64>,
    fiat_amount: i64,
) -> bool {
    matches!((is_mine, min, max), (Some(true), Some(min), Some(max)) if max - fiat_amount >= min)
}

/// Whose order a saved row is: the one the user made with `order new`, or the
/// one the user took with `order take-*`. Only a maker continues a range order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Maker,
    Taker,
}

/// A range order the user made, with room left in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MakerRange {
    /// The user sells (a `sell` order); otherwise the user buys.
    pub sells: bool,
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

    /// The trade key and index the next reservation would get: the recorded
    /// index plus one. Writes nothing. Only for the human preview: a request
    /// publishes the trade of [`Store::reserve_trade`], which is this one unless
    /// another command reserved one meanwhile.
    pub async fn next_trade(&self) -> Result<Trade, StoreError> {
        let pool = self.open().await?;
        let trade = User::get_next_trade_keys(&pool).await;
        pool.close().await;
        trade
            .map(|(keys, index)| Trade { index, keys })
            .map_err(|_| StoreError::Unwritable)
    }

    /// Reserve the next trade right before publishing: its index is recorded as
    /// spent, and committed, before this returns. Call it after every local
    /// check and the confirmation; never hold it across a prompt.
    pub async fn reserve_trade(&self) -> Result<Reservation, StoreError> {
        let pool = self.open().await?;
        let reserved = reserve_in(&pool).await;
        pool.close().await;
        reserved
    }

    /// Give `reservation` back once its request never left or the node
    /// refused it (see `commands::order::write` for the refusal it keeps).
    /// Best effort: `false` leaves the index spent, a harmless gap.
    pub async fn release(&self, reservation: &Reservation) -> bool {
        let Ok(pool) = self.open().await else {
            return false;
        };
        let released = release_in(&pool, reservation).await;
        pool.close().await;
        released
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
        let pool = self.open_read_only().await?;
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

    /// What `fiat-sent` and `release` need to know about the stored order `id`
    /// when it is a range order the user made with room left in it: `None` for
    /// anything else (a taken order, a fixed order, an exhausted range, an unknown
    /// order). Writes nothing.
    pub async fn maker_range(&self, id: Uuid) -> Result<Option<MakerRange>, StoreError> {
        let pool = self.open().await?;
        let row: Result<Option<RangeRow>, _> = sqlx::query_as(
            "SELECT is_mine, min_amount, max_amount, fiat_amount, kind FROM orders WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&pool)
        .await;
        pool.close().await;
        let row = row.map_err(|_| StoreError::Unwritable)?;
        Ok(row.and_then(|(is_mine, min, max, fiat, kind)| {
            range_has_room(is_mine, min, max, fiat).then(|| MakerRange {
                sells: kind.as_deref() == Some("sell"),
            })
        }))
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

    /// Save the order the user made and record its index, in one transaction:
    /// both are written or neither is.
    pub async fn save_order(
        &self,
        order: &SmallOrder,
        trade: &Trade,
        request_id: u64,
    ) -> Result<(), StoreError> {
        self.save_order_as(order, trade, request_id, Role::Maker)
            .await
    }

    /// [`Store::save_order`] for an order of the given `role`.
    pub async fn save_order_as(
        &self,
        order: &SmallOrder,
        trade: &Trade,
        request_id: u64,
        role: Role,
    ) -> Result<(), StoreError> {
        let pool = self.open().await?;
        let result = save_in(&pool, order, trade, request_id, (role, Existing::Replace)).await;
        pool.close().await;
        result.map(|_| ())
    }

    /// Save `order` as the user's own under `trade` only if no row has its id:
    /// `true` when a row was written, `false` when one existed and was left
    /// untouched. The index is recorded in the same transaction.
    pub async fn adopt_order(
        &self,
        order: &SmallOrder,
        trade: &Trade,
        request_id: u64,
    ) -> Result<bool, StoreError> {
        let pool = self.open().await?;
        let result = save_in(
            &pool,
            order,
            trade,
            request_id,
            (Role::Maker, Existing::Keep),
        )
        .await;
        pool.close().await;
        result
    }

    /// The ids of every order in the store. Writes nothing.
    pub async fn known_orders(&self) -> Result<std::collections::HashSet<Uuid>, StoreError> {
        let pool = self.open().await?;
        let ids: Result<Vec<String>, _> = sqlx::query_scalar("SELECT id FROM orders")
            .fetch_all(&pool)
            .await;
        pool.close().await;
        let ids = ids.map_err(|_| StoreError::Unreadable)?;
        Ok(ids
            .iter()
            .filter_map(|id| Uuid::parse_str(id).ok())
            .collect())
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
