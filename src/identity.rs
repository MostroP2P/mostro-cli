//! Read-only access to the local identity.
//!
//! Commands that only ask the node about the user's own data need the identity
//! key and nothing else. This module opens the existing `mcli.db` read-only and
//! derives that key. It never creates a directory or a file, never changes a
//! permission, never migrates and never writes, so a missing or damaged store is
//! reported instead of repaired. Errors carry no text from the store.

use std::fmt;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use mostro_core::prelude::NOSTR_ORDER_EVENT_KIND;
use nostr_sdk::prelude::{FromMnemonic, Keys};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// Why no identity key could be produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// No data directory, no `mcli.db`, or no user row in it.
    Missing,
    /// The store exists but cannot be read or holds no usable mnemonic.
    Unreadable,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "no local identity exists",
            Self::Unreadable => "the local identity store cannot be read",
        })
    }
}

impl std::error::Error for IdentityError {}

/// The identity key of `mnemonic`: account `NOSTR_ORDER_EVENT_KIND`, change 0,
/// index 0. The only derivation of this key in the program. An invalid
/// mnemonic is `Unreadable`; its text is never part of the error.
pub fn identity_keys_from_mnemonic(mnemonic: &str) -> Result<Keys, IdentityError> {
    trade_keys_from_mnemonic(mnemonic, 0)
}

/// The last trade index a key can be derived for: the address is a normal
/// (non-hardened) BIP32 index, below 2^31.
pub const LAST_TRADE_INDEX: i64 = i32::MAX as i64;

/// The trade key of `index` of `mnemonic`: account `NOSTR_ORDER_EVENT_KIND`,
/// change 0, address `index` (0 is the identity). The only derivation of a
/// trade key in the program. An invalid mnemonic or an index outside
/// `0..=LAST_TRADE_INDEX` is `Unreadable`; the mnemonic is never part of the
/// error.
pub fn trade_keys_from_mnemonic(mnemonic: &str, index: i64) -> Result<Keys, IdentityError> {
    if !(0..=LAST_TRADE_INDEX).contains(&index) {
        return Err(IdentityError::Unreadable);
    }
    let index = u32::try_from(index).map_err(|_| IdentityError::Unreadable)?;
    let account = NOSTR_ORDER_EVENT_KIND as u32;
    Keys::from_mnemonic_advanced(mnemonic, None, Some(account), Some(0), Some(index))
        .map_err(|_| IdentityError::Unreadable)
}

/// The data directory without creating it: a non-blank `MCLI_DIR`, else
/// `<home>/.mcli`. `None` when neither can be determined.
pub fn resolve_dir(mcli_dir: Option<&str>, home: Option<PathBuf>) -> Option<PathBuf> {
    match mcli_dir.map(str::trim).filter(|dir| !dir.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => home.map(|home| home.join(".mcli")),
    }
}

/// The configured data directory (`MCLI_DIR`, else `<home>/.mcli`), without
/// creating it.
pub fn configured_dir() -> Option<PathBuf> {
    let env = std::env::var("MCLI_DIR").ok();
    resolve_dir(env.as_deref(), dirs::home_dir())
}

/// The identity key from the store in the configured data directory.
pub async fn load_identity_keys() -> Result<Keys, IdentityError> {
    let dir = configured_dir().ok_or(IdentityError::Missing)?;
    load_identity_keys_from(&dir).await
}

/// How the store is opened so that SQLite creates and deletes no file beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenMode {
    /// An ordinary read-only open: a rollback-journal store without a `-wal`
    /// file, or a store whose `-wal` and `-shm` files both exist.
    ReadOnly,
    /// The main file alone, as immutable: a WAL store without a `-wal` file
    /// holds everything in the main file, and an ordinary open would create the
    /// `-wal` and `-shm` files.
    Immutable,
    /// The main file alone, as immutable, of a store whose `-wal` file has no
    /// `-shm` file: an ordinary open would create the `-shm` file to read the
    /// WAL. The mnemonic never changes once written, so a key found in the main
    /// file is the right one; a user row that may exist only in the WAL is
    /// reported unreadable, not missing.
    MainFileOnly,
}

/// How `path` can be read without a file appearing or disappearing beside it.
/// `None` when it is too short to hold a database header: SQLite would delete
/// the `-wal` file of an empty main file.
pub(crate) fn open_mode(path: &Path) -> Option<OpenMode> {
    let sidecar = |suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name).exists()
    };
    let mut header = [0u8; 20];
    std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut header))
        .ok()?;
    let wal_mode = header[18] == 2 && header[19] == 2;
    Some(match (sidecar("-wal"), sidecar("-shm")) {
        (true, true) => OpenMode::ReadOnly,
        (true, false) => OpenMode::MainFileOnly,
        (false, _) if wal_mode => OpenMode::Immutable,
        (false, _) => OpenMode::ReadOnly,
    })
}

/// The identity key from `dir/mcli.db`, opened read-only.
pub async fn load_identity_keys_from(dir: &Path) -> Result<Keys, IdentityError> {
    let (pool, mode) = open_store(dir).await?;
    let keys = read_keys(&pool, mode).await;
    pool.close().await;
    keys
}

/// The identity key and the last trade index recorded with it (`0` when none
/// is), from `dir/mcli.db` opened read-only. Unlike the mnemonic the index
/// changes, so a store whose WAL cannot be read without creating a file
/// beside it is `Unreadable`, and so is an index outside
/// `0..=LAST_TRADE_INDEX`.
pub async fn load_identity_from(dir: &Path) -> Result<(Keys, i64), IdentityError> {
    let (pool, mode) = open_store(dir).await?;
    let keys = read_keys(&pool, mode).await;
    let index = match (&keys, mode) {
        (Err(_), _) => Ok(0),
        (Ok(_), OpenMode::MainFileOnly) => Err(IdentityError::Unreadable),
        (Ok(_), _) => read_index(&pool).await,
    };
    pool.close().await;
    Ok((keys?, index?))
}

/// The recorded last trade index of the first user, `0` when none.
async fn read_index(pool: &SqlitePool) -> Result<i64, IdentityError> {
    let index = sqlx::query_scalar::<_, Option<i64>>("SELECT last_trade_index FROM users LIMIT 1")
        .fetch_one(pool)
        .await
        .map_err(|_| IdentityError::Unreadable)?
        .unwrap_or(0);
    if (0..=LAST_TRADE_INDEX).contains(&index) {
        Ok(index)
    } else {
        Err(IdentityError::Unreadable)
    }
}

/// Open `dir/mcli.db` read-only so that no file appears or disappears beside
/// it, and say how it was opened.
async fn open_store(dir: &Path) -> Result<(SqlitePool, OpenMode), IdentityError> {
    let path = dir.join("mcli.db");
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(IdentityError::Unreadable),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Err(IdentityError::Missing)
        }
        Err(_) => return Err(IdentityError::Unreadable),
    }
    // A read-only open of a WAL store makes SQLite create a missing `-wal` or
    // `-shm` file, so a store without both is read as immutable, which touches
    // nothing (see `OpenMode`). The mnemonic never changes after the first run,
    // so a reader that ignores a WAL cannot return a wrong key.
    let mode = open_mode(&path).ok_or(IdentityError::Unreadable)?;
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .read_only(true)
        .immutable(mode != OpenMode::ReadOnly)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|_| IdentityError::Unreadable)?;
    Ok((pool, mode))
}

/// The identity key of the first user's mnemonic.
async fn read_keys(pool: &SqlitePool, mode: OpenMode) -> Result<Keys, IdentityError> {
    let row = sqlx::query_scalar::<_, Option<String>>("SELECT mnemonic FROM users LIMIT 1")
        .fetch_optional(pool)
        .await;
    match row {
        Err(_) | Ok(Some(None)) => Err(IdentityError::Unreadable),
        Ok(None) if mode == OpenMode::MainFileOnly => Err(IdentityError::Unreadable),
        Ok(None) => Err(IdentityError::Missing),
        Ok(Some(Some(mnemonic))) if mnemonic.trim().is_empty() => Err(IdentityError::Unreadable),
        Ok(Some(Some(mnemonic))) => identity_keys_from_mnemonic(&mnemonic),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::User;
    use sqlx::sqlite::SqliteJournalMode;
    use sqlx::SqlitePool;
    use std::collections::hash_map::DefaultHasher;
    use std::fs;
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A unique directory under the system temp dir, removed on drop.
    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let seq = SEQ.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("mcli-identity-{}-{seq}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn db(&self) -> PathBuf {
            self.0.join("mcli.db")
        }

        /// Name, size and content hash of every file, to compare before and
        /// after a load.
        fn snapshot(&self) -> Vec<(String, usize, u64)> {
            let mut files: Vec<_> = fs::read_dir(&self.0)
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    let bytes = fs::read(entry.path()).unwrap_or_default();
                    let mut hasher = DefaultHasher::new();
                    bytes.hash(&mut hasher);
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        bytes.len(),
                        hasher.finish(),
                    )
                })
                .collect();
            files.sort();
            files
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    async fn store(dir: &Dir, schema: &'static str) -> SqlitePool {
        store_in(dir, schema, SqliteJournalMode::Delete).await
    }

    async fn store_in(dir: &Dir, schema: &'static str, mode: SqliteJournalMode) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(dir.db())
            .create_if_missing(true)
            .journal_mode(mode);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::query(schema).execute(&pool).await.unwrap();
        pool
    }

    const USERS: &str = "CREATE TABLE users (i0_pubkey char(64) PRIMARY KEY, mnemonic TEXT, \
                         last_trade_index INTEGER, created_at INTEGER)";

    async fn valid_store(dir: &Dir) -> Keys {
        valid_store_in(dir, SqliteJournalMode::Delete).await
    }

    /// A store at rest in `mode`. A WAL store is written in rollback mode and
    /// then marked WAL in its header (bytes 18 and 19 set to 2), the state a
    /// WAL writer leaves after a clean close. Closing a real WAL writer only
    /// removes its `-wal` and `-shm` files when SQLite's close-time checkpoint
    /// gets an exclusive lock, and under a loaded CI run it once did not.
    async fn valid_store_in(dir: &Dir, mode: SqliteJournalMode) -> Keys {
        let wal = matches!(mode, SqliteJournalMode::Wal);
        let seed_mode = if wal { SqliteJournalMode::Delete } else { mode };
        let pool = store_in(dir, USERS, seed_mode).await;
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        User::new(mnemonic, &pool).await.unwrap();
        let keys = User::get_identity_keys(&pool).await.unwrap();
        pool.close().await;
        if wal {
            let mut bytes = fs::read(dir.db()).unwrap();
            bytes[18..20].copy_from_slice(&[2, 2]);
            fs::write(dir.db(), bytes).unwrap();
        }
        keys
    }

    #[test]
    fn the_directory_is_resolved_without_creating_it() {
        let home = Some(PathBuf::from("/home/someone"));
        assert_eq!(
            resolve_dir(Some(" /data/mcli "), home.clone()),
            Some(PathBuf::from("/data/mcli"))
        );
        for blank in [None, Some(""), Some("   ")] {
            assert_eq!(
                resolve_dir(blank, home.clone()),
                Some(PathBuf::from("/home/someone/.mcli"))
            );
        }
        assert_eq!(resolve_dir(None, None), None);
        assert!(!Path::new("/home/someone/.mcli").exists());
    }

    #[tokio::test]
    async fn a_missing_directory_or_file_is_missing_and_nothing_is_created() {
        let dir = Dir::new();
        let absent = dir.0.join("never-created");
        assert_eq!(
            load_identity_keys_from(&absent).await.unwrap_err(),
            IdentityError::Missing
        );
        assert!(!absent.exists());

        assert_eq!(
            load_identity_keys_from(&dir.0).await.unwrap_err(),
            IdentityError::Missing
        );
        assert!(dir.snapshot().is_empty(), "a sibling file appeared");
    }

    #[tokio::test]
    async fn a_store_without_a_user_row_is_missing() {
        let dir = Dir::new();
        store(&dir, USERS).await.close().await;
        let before = dir.snapshot();
        assert_eq!(
            load_identity_keys_from(&dir.0).await.unwrap_err(),
            IdentityError::Missing
        );
        assert_eq!(dir.snapshot(), before);
    }

    #[tokio::test]
    async fn damaged_stores_are_unreadable_and_unchanged() {
        let garbage = Dir::new();
        fs::write(
            garbage.db(),
            b"this is not a sqlite database at all, just text",
        )
        .unwrap();

        let no_table = Dir::new();
        store(&no_table, "CREATE TABLE other (x INTEGER)")
            .await
            .close()
            .await;

        let null_mnemonic = Dir::new();
        let pool = store(&null_mnemonic, USERS).await;
        sqlx::query("INSERT INTO users (i0_pubkey, mnemonic) VALUES ('aa', NULL)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let bad_mnemonic = Dir::new();
        let pool = store(&bad_mnemonic, USERS).await;
        sqlx::query("INSERT INTO users (i0_pubkey, mnemonic) VALUES ('aa', 'definitely not a mnemonic phrase')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        for (label, dir) in [
            ("garbage", &garbage),
            ("no users table", &no_table),
            ("null mnemonic", &null_mnemonic),
            ("invalid mnemonic", &bad_mnemonic),
        ] {
            let before = dir.snapshot();
            let error = load_identity_keys_from(&dir.0).await.unwrap_err();
            assert_eq!(error, IdentityError::Unreadable, "{label}");
            assert_eq!(dir.snapshot(), before, "{label}: the store changed");
            let text = format!("{error} {error:?}");
            assert!(
                !text.contains("mnemonic phrase") && !text.contains("sqlite"),
                "{label}: {text}"
            );
        }
    }

    #[tokio::test]
    async fn a_valid_store_yields_the_production_key_and_is_not_touched() {
        let dir = Dir::new();
        let expected = valid_store(&dir).await;
        let before = dir.snapshot();

        let keys = load_identity_keys_from(&dir.0).await.unwrap();

        assert_eq!(keys.public_key(), expected.public_key());
        assert_eq!(dir.snapshot(), before, "loading changed the directory");
    }

    /// An older build may have left the store in WAL mode. A read-only open of
    /// such a store must work without leaving `-wal` or `-shm` files behind or
    /// changing the main file.
    #[tokio::test]
    async fn a_wal_mode_store_is_read_without_sidecar_files() {
        let dir = Dir::new();
        let expected = valid_store_in(&dir, SqliteJournalMode::Wal).await;
        let header = fs::read(dir.db()).unwrap();
        assert_eq!(
            &header[18..20],
            &[2, 2],
            "the seeded store is not in WAL mode"
        );
        let before = dir.snapshot();
        assert_eq!(
            before
                .iter()
                .map(|(name, ..)| name.as_str())
                .collect::<Vec<_>>(),
            ["mcli.db"],
            "the writer left sidecar files"
        );

        for _ in 0..3 {
            let keys = load_identity_keys_from(&dir.0).await.unwrap();
            assert_eq!(keys.public_key(), expected.public_key());
        }

        assert_eq!(dir.snapshot(), before);
    }

    /// Copies `names` from a live store into a new directory while its writer
    /// is still open, as a backup that skipped the `-shm` file would.
    fn copy_of(live: &Dir, names: &[&str]) -> Dir {
        let copy = Dir::new();
        for name in names {
            fs::write(copy.0.join(name), fs::read(live.0.join(name)).unwrap()).unwrap();
        }
        copy
    }

    /// A WAL store copied without its `-shm` file, whose user row lives only in
    /// `mcli.db-wal`: an ordinary read-only open would create `mcli.db-shm`,
    /// and the main file alone has no user row. The store is reported
    /// unreadable, not missing, and left exactly as it was.
    #[tokio::test]
    async fn a_wal_file_without_its_shm_file_is_unreadable_when_the_row_is_only_in_the_wal() {
        let live = Dir::new();
        let pool = store_in(&live, USERS, SqliteJournalMode::Wal).await;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await
            .unwrap();
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        User::new(mnemonic, &pool).await.unwrap();
        let copy = copy_of(&live, &["mcli.db", "mcli.db-wal"]);
        pool.close().await;
        let before = copy.snapshot();

        let result = load_identity_keys_from(&copy.0).await;

        assert_eq!(copy.snapshot(), before, "loading changed the directory");
        assert_eq!(result.unwrap_err(), IdentityError::Unreadable);
    }

    /// The same copy when the user row was already checkpointed into the main
    /// file and the WAL holds only later writes: the key is read from the main
    /// file and nothing is created.
    #[tokio::test]
    async fn a_wal_file_without_its_shm_file_yields_the_key_in_the_main_file() {
        let live = Dir::new();
        let pool = store_in(&live, USERS, SqliteJournalMode::Wal).await;
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        User::new(mnemonic, &pool).await.unwrap();
        let expected = User::get_identity_keys(&pool).await.unwrap();
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET last_trade_index = 5")
            .execute(&pool)
            .await
            .unwrap();
        let copy = copy_of(&live, &["mcli.db", "mcli.db-wal"]);
        pool.close().await;
        let before = copy.snapshot();
        assert!(before[1].1 > 0, "the WAL holds no frame: {before:?}");

        let keys = load_identity_keys_from(&copy.0).await.unwrap();

        assert_eq!(keys.public_key(), expected.public_key());
        assert_eq!(copy.snapshot(), before, "loading changed the directory");
    }

    /// An empty `-wal` file without its `-shm` file, beside a WAL-mode or a
    /// rollback-journal store: the key is read and nothing is created.
    #[tokio::test]
    async fn an_empty_wal_file_without_its_shm_file_is_read_without_changes() {
        for mode in [SqliteJournalMode::Wal, SqliteJournalMode::Delete] {
            let dir = Dir::new();
            let expected = valid_store_in(&dir, mode).await;
            fs::write(dir.0.join("mcli.db-wal"), b"").unwrap();
            let before = dir.snapshot();

            let keys = load_identity_keys_from(&dir.0).await.unwrap();

            assert_eq!(keys.public_key(), expected.public_key(), "{mode:?}");
            assert_eq!(
                dir.snapshot(),
                before,
                "{mode:?}: loading changed the directory"
            );
        }
    }

    /// A `-shm` file without its `-wal` file: nothing lives outside the main
    /// file, which is read as immutable. An ordinary read-only open would
    /// create `mcli.db-wal` and rewrite the `-shm` file.
    #[tokio::test]
    async fn a_shm_file_without_its_wal_file_is_read_without_creating_one() {
        let dir = Dir::new();
        let expected = valid_store_in(&dir, SqliteJournalMode::Wal).await;
        fs::write(dir.0.join("mcli.db-shm"), [0u8; 32768]).unwrap();
        let before = dir.snapshot();

        let keys = load_identity_keys_from(&dir.0).await.unwrap();

        assert_eq!(keys.public_key(), expected.public_key());
        assert_eq!(dir.snapshot(), before, "loading changed the directory");
    }

    /// SQLite deletes the `-wal` file of an empty main file when it opens it,
    /// so such a store is reported unreadable without being opened.
    #[tokio::test]
    async fn an_empty_main_file_keeps_its_wal_file() {
        let live = Dir::new();
        let pool = store_in(&live, USERS, SqliteJournalMode::Wal).await;
        let wal = fs::read(live.0.join("mcli.db-wal")).unwrap();
        pool.close().await;
        let dir = Dir::new();
        fs::write(dir.db(), b"").unwrap();
        fs::write(dir.0.join("mcli.db-wal"), wal).unwrap();
        fs::write(dir.0.join("mcli.db-shm"), [0u8; 32768]).unwrap();
        let before = dir.snapshot();

        let result = load_identity_keys_from(&dir.0).await;

        assert_eq!(dir.snapshot(), before, "loading changed the directory");
        assert_eq!(result.unwrap_err(), IdentityError::Unreadable);
    }

    /// While another process has the store open, its `-wal` and `-shm` files
    /// exist; reading must neither remove nor add any file nor change the main one.
    #[tokio::test]
    async fn a_wal_store_in_use_by_a_writer_is_read_without_changes() {
        let dir = Dir::new();
        let pool = store_in(&dir, USERS, SqliteJournalMode::Wal).await;
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        User::new(mnemonic, &pool).await.unwrap();
        let expected = User::get_identity_keys(&pool).await.unwrap();
        let before = dir.snapshot();
        assert!(
            before.len() >= 2,
            "the writer should have sidecar files: {before:?}"
        );

        let keys = load_identity_keys_from(&dir.0).await.unwrap();

        assert_eq!(keys.public_key(), expected.public_key());
        let after = dir.snapshot();
        let names =
            |files: &[(String, usize, u64)]| files.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
        assert_eq!(names(&after), names(&before));
        pool.close().await;
    }

    #[tokio::test]
    async fn the_identity_comes_with_its_last_trade_index_and_nothing_changes() {
        let dir = Dir::new();
        let expected = valid_store(&dir).await;
        let before = dir.snapshot();
        let (keys, index) = load_identity_from(&dir.0).await.unwrap();
        assert_eq!(keys.public_key(), expected.public_key());
        assert_eq!(index, 0, "no recorded index reads as 0");
        assert_eq!(dir.snapshot(), before);

        for recorded in [1, 7, LAST_TRADE_INDEX] {
            let pool = store(&dir, "SELECT 1").await;
            sqlx::query("UPDATE users SET last_trade_index = ?")
                .bind(recorded)
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
            let (_, index) = load_identity_from(&dir.0).await.unwrap();
            assert_eq!(index, recorded);
        }
    }

    #[tokio::test]
    async fn an_index_out_of_range_is_unreadable() {
        for recorded in [-1, LAST_TRADE_INDEX + 1] {
            let dir = Dir::new();
            valid_store(&dir).await;
            let pool = store(&dir, "SELECT 1").await;
            sqlx::query("UPDATE users SET last_trade_index = ?")
                .bind(recorded)
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
            let before = dir.snapshot();
            assert_eq!(
                load_identity_from(&dir.0).await.unwrap_err(),
                IdentityError::Unreadable,
                "{recorded}"
            );
            assert_eq!(dir.snapshot(), before);
            // The key alone is still readable.
            assert!(load_identity_keys_from(&dir.0).await.is_ok());
        }
    }

    #[tokio::test]
    async fn missing_and_damaged_stores_fail_the_same_way_as_the_key() {
        let empty = Dir::new();
        assert_eq!(
            load_identity_from(&empty.0).await.unwrap_err(),
            IdentityError::Missing
        );
        let no_user = Dir::new();
        store(&no_user, USERS).await.close().await;
        assert_eq!(
            load_identity_from(&no_user.0).await.unwrap_err(),
            IdentityError::Missing
        );
        let garbage = Dir::new();
        fs::write(garbage.db(), b"this is not a sqlite database at all").unwrap();
        assert_eq!(
            load_identity_from(&garbage.0).await.unwrap_err(),
            IdentityError::Unreadable
        );
        assert!(empty.snapshot().is_empty());
    }

    /// A WAL copied without its `-shm` file may hold a newer index than the
    /// main file: the index is unreadable there, though the key is not.
    #[tokio::test]
    async fn a_wal_without_its_shm_file_has_no_readable_index() {
        let live = Dir::new();
        let pool = store_in(&live, USERS, SqliteJournalMode::Wal).await;
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        User::new(mnemonic, &pool).await.unwrap();
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET last_trade_index = 5")
            .execute(&pool)
            .await
            .unwrap();
        let copy = copy_of(&live, &["mcli.db", "mcli.db-wal"]);
        pool.close().await;
        let before = copy.snapshot();

        assert_eq!(
            load_identity_from(&copy.0).await.unwrap_err(),
            IdentityError::Unreadable
        );
        assert!(load_identity_keys_from(&copy.0).await.is_ok());
        assert_eq!(copy.snapshot(), before, "loading changed the directory");
    }

    #[test]
    fn the_shared_derivation_matches_the_database_layer() {
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        let keys = identity_keys_from_mnemonic(&mnemonic).unwrap();
        let again = identity_keys_from_mnemonic(&mnemonic).unwrap();
        assert_eq!(keys.public_key(), again.public_key());
        assert!(identity_keys_from_mnemonic("not a mnemonic").is_err());
    }

    #[test]
    fn a_trade_key_is_the_address_of_its_index_and_index_zero_is_the_identity() {
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        let account = NOSTR_ORDER_EVENT_KIND as u32;
        let seventh =
            Keys::from_mnemonic_advanced(&mnemonic, None, Some(account), Some(0), Some(7)).unwrap();
        assert_eq!(
            trade_keys_from_mnemonic(&mnemonic, 7).unwrap().public_key(),
            seventh.public_key()
        );
        assert_eq!(
            trade_keys_from_mnemonic(&mnemonic, 0).unwrap().public_key(),
            identity_keys_from_mnemonic(&mnemonic).unwrap().public_key()
        );
        for outside in [-1, LAST_TRADE_INDEX + 1, i64::from(u32::MAX)] {
            assert_eq!(
                trade_keys_from_mnemonic(&mnemonic, outside).unwrap_err(),
                IdentityError::Unreadable,
                "{outside}"
            );
        }
        assert!(trade_keys_from_mnemonic(&mnemonic, LAST_TRADE_INDEX).is_ok());
    }
}
