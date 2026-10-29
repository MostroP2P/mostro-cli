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
    let account = NOSTR_ORDER_EVENT_KIND as u32;
    Keys::from_mnemonic_advanced(mnemonic, None, Some(account), Some(0), Some(0))
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

/// The identity key from the store in the configured data directory.
pub async fn load_identity_keys() -> Result<Keys, IdentityError> {
    let env = std::env::var("MCLI_DIR").ok();
    let dir = resolve_dir(env.as_deref(), dirs::home_dir()).ok_or(IdentityError::Missing)?;
    load_identity_keys_from(&dir).await
}

/// Whether `path` is a WAL-mode database with no `-wal` or `-shm` file beside it.
fn is_wal_without_sidecars(path: &Path) -> bool {
    let sidecar = |suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name).exists()
    };
    let mut header = [0u8; 20];
    let wal_mode = std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut header))
        .is_ok()
        && header[18] == 2
        && header[19] == 2;
    wal_mode && !sidecar("-wal") && !sidecar("-shm")
}

/// The identity key from `dir/mcli.db`, opened read-only.
pub async fn load_identity_keys_from(dir: &Path) -> Result<Keys, IdentityError> {
    let path = dir.join("mcli.db");
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(IdentityError::Unreadable),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Err(IdentityError::Missing)
        }
        Err(_) => return Err(IdentityError::Unreadable),
    }
    // A read-only open of a WAL store makes SQLite create `-wal` and `-shm`
    // files. When none exist yet, open it as immutable instead, which touches
    // nothing. The mnemonic never changes after the first run, so a reader that
    // ignores a live WAL cannot return a wrong key; with sidecars present a
    // normal read-only open creates nothing new.
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .read_only(true)
        .immutable(is_wal_without_sidecars(&path))
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|_| IdentityError::Unreadable)?;
    let row = sqlx::query_scalar::<_, Option<String>>("SELECT mnemonic FROM users LIMIT 1")
        .fetch_optional(&pool)
        .await;
    pool.close().await;
    match row {
        Err(_) | Ok(Some(None)) => Err(IdentityError::Unreadable),
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

    async fn valid_store_in(dir: &Dir, mode: SqliteJournalMode) -> Keys {
        let pool = store_in(dir, USERS, mode).await;
        let mnemonic = bip39::Mnemonic::generate(bip39::WordCount::Words12)
            .unwrap()
            .to_string();
        User::new(mnemonic, &pool).await.unwrap();
        let keys = User::get_identity_keys(&pool).await.unwrap();
        pool.close().await;
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
}
